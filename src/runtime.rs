extern crate libc;

use tokio;
use tokio::prelude::*;

use std::ffi::CString;
use std::sync::{Mutex, Once};

use std::fs::File;
use std::io::Read;

use libfly::*;

use futures::sync::oneshot;
use std::collections::HashMap;

use std::thread;
use tokio::runtime::current_thread;

use tokio::timer::{Delay, Interval};

use std::time::{Duration, Instant};

use futures::future;

use messages::JsValue;
use messages::{Command, Message, TimerReady};

extern crate libfly;

extern crate hyper;

#[derive(Debug, Copy, Clone)]
pub struct JsRuntime(pub *const js_runtime);
unsafe impl Send for JsRuntime {}
unsafe impl Sync for JsRuntime {}

impl JsRuntime {
  pub fn send(&self, kind: MessageKind, cmd: Value) -> Value {
    // let ptr = args.as_ptr();
    // let len = args.len() as i32;
    unsafe {
      // mem::forget(args);
      // let n = name.to_string();
      // let namestr = CString::new(name.as_str()).unwrap();
      js_send(self.0, kind, cmd)
    }
  }
}

#[derive(Debug)]
pub struct Runtime {
  pub ptr: JsRuntime,
  pub rt: Mutex<tokio::runtime::current_thread::Handle>,
  timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
  pub responses: Mutex<HashMap<i32, oneshot::Sender<Vec<Value>>>>,
}

static JSINIT: Once = Once::new();

impl Runtime {
  pub fn new() -> Box<Self> {
    JSINIT.call_once(|| unsafe {
      js_init(
        fly_buf {
          ptr: NATIVES_DATA.as_ptr() as *const i8,
          len: NATIVES_DATA.len(),
        },
        fly_buf {
          ptr: SNAPSHOT_DATA.as_ptr() as *const i8,
          len: SNAPSHOT_DATA.len(),
        },
      )
    });

    let (c, p) = oneshot::channel::<current_thread::Handle>();
    thread::spawn(move || {
      let mut l = current_thread::Runtime::new().unwrap();
      let task = Interval::new_interval(Duration::from_secs(5))
        .for_each(move |_| {
          // println!("keepalive");
          Ok(())
        }).map_err(|e| panic!("interval errored; err={:?}", e));
      l.spawn(task);
      match c.send(l.handle()) {
        Ok(_) => println!("sent event loop handle fine"),
        Err(e) => panic!(e),
      };

      l.run()
    });

    let mut rt_box = Box::new(Runtime {
      ptr: JsRuntime(0 as *const js_runtime),
      rt: Mutex::new(p.wait().unwrap()),
      timers: Mutex::new(HashMap::new()),
      responses: Mutex::new(HashMap::new()),
    });

    (*rt_box).ptr.0 = unsafe {
      let ptr = js_runtime_new(
        *FLY_SNAPSHOT,
        rt_box.as_ref() as *const _ as *mut libc::c_void,
      );
      js_eval(
        ptr,
        CString::new("fly_main.js").unwrap().as_ptr(),
        CString::new("flyMain()").unwrap().as_ptr(),
      );
      ptr
    };

    rt_box
  }

  pub fn eval(&self, filename: &str, code: &str) {
    unsafe {
      js_eval(
        self.ptr.0,
        CString::new(filename).unwrap().as_ptr(),
        CString::new(code).unwrap().as_ptr(),
      )
    };
  }

  pub fn eval_file(&self, filename: &str) {
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    self.eval(filename, contents.as_str());
  }

  pub fn used_heap_size(&self) -> usize {
    unsafe { js_runtime_heap_statistics(self.ptr.0) }.used_heap_size as usize
  }

  pub fn send(&self, kind: MessageKind, cmd: Command) -> Value {
    // let tmp = TempValue::Object{}
    self.ptr.send(kind, cmd.prepare_js().to_js())
  }
}

pub fn from_c<'a>(rt: *const js_runtime) -> &'a mut Runtime {
  let ptr = unsafe { js_get_data(rt) };
  let rt_ptr = ptr as *mut Runtime;
  let rt_box = unsafe { Box::from_raw(rt_ptr) };
  Box::leak(rt_box)
}

const NATIVES_DATA: &'static [u8] =
  include_bytes!("../libfly/third_party/v8/out.gn/x64.debug/natives_blob.bin");
const SNAPSHOT_DATA: &'static [u8] =
  include_bytes!("../libfly/third_party/v8/out.gn/x64.debug/snapshot_blob.bin");

lazy_static! {
  static ref FLY_SNAPSHOT: fly_buf = unsafe {
    let filename = "fly/packages/v8env/dist/bundle.js";
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    js_create_snapshot(
      CString::new(filename).unwrap().as_ptr(),
      CString::new(contents).unwrap().as_ptr(),
    )
  };
}

// Buf represents a byte array returned from a "Op".
// The message might be empty (which will be translated into a null object on
// the javascript side) or it is a heap allocated opaque sequence of bytes.
// Usually a flatbuffer message.
type Buf = Option<Command>;

// JS promises in Deno map onto a specific Future
// which yields either a DenoError or a byte array.
type Op = Future<Item = Buf, Error = String> + Send;

type OpResult = Result<Buf, String>;

type HandlerResult = Box<Op>;
// type Handler = fn(rt: &Runtime, base: msg::Base) -> HandlerResult;

// pub struct Message {
//   cmd_id: i32,
//   name: String,
//   args: Vec<Value>,
// }

#[no_mangle]
pub extern "C" fn msg_from_js(
  raw: *const js_runtime,
  msg: libfly::Message,
  // cmd_id: libc::c_int,
  // name: *mut libc::c_char,
  // sync: bool,
  // argc: libc::c_int,
  // argv: *mut Value,
) {
  println!("got msg from js! {:?} payload: {:?}", msg, msg.payload());

  let sync = msg.sync;
  let payload = msg.payload().unwrap(); // TODO: this might panic

  // let name = unsafe {
  //   // let n = CString::new(slice::from_raw_parts(name.ptr, name.len)).unwrap();
  //   let n = CString::from_raw(name);
  //   n.into_string().unwrap()
  // };

  // println!("name: {}", name);

  // let args = unsafe { Vec::from_raw_parts(argv, argc as usize, argc as usize) };

  // // let val = unsafe { js_current_arg_value(raw, 0) };

  // // println!("arr: {:?}", args);
  // // let v = Value::Int32(10);

  // // unsafe { js_set_return_value(raw, &v) };

  let rt = from_c(raw);

  // if name.as_str() == "http_response" {
  //   // println!("got an http response!");
  //   // let builder = &mut FlatBufferBuilder::new();
  //   let mut responses = rt.responses.lock().unwrap();
  //   let sender = responses.remove(&cmd_id).unwrap();
  //   sender.send(args).unwrap();
  //   // unsafe { js_set_response(rt.ptr.0, null_buf()) };
  //   return;
  // }

  let handler = match msg.kind {
    MessageKind::TimerStart => handle_timer_start,
    _ => unimplemented!(),
  };
  //   "timer_start" => handle_timer_start,
  //   "timer_clear" => handle_timer_clear,
  //   // "decode" => handle_decode,
  //   _ => panic!("ahhh, unhandled event"),
  // };

  let fut = handler(rt, msg.id, payload);

  // if sync {
  //   match fut.wait().unwrap() {
  //     None => {}
  //     Some(box_cmd) => {
  //       let cmd = Box::into_raw(box_cmd);
  //       // let cmd = *ptr;
  //       unsafe { js_set_return_value(rt.ptr.0, (*cmd).prepare_js().to_js()) };
  //     }
  //   }
  // } else {
  let ptr = rt.ptr;
  // Execute future asynchornously.
  rt.rt
    .lock()
    .unwrap()
    .spawn(
      fut
        .map_err(|e: String| println!("ERROR SPAWNING SHIT: {}", e))
        .and_then(move |maybe_cmd| {
          println!("handler future and_then");
          match maybe_cmd {
            Some(cmd) => {
              // let cmd = Box::leak(box_cmd);
              unsafe {
                let tmp = cmd.prepare_js();
                println!("tmp: {:?}", tmp);
                js_send(ptr.0, MessageKind::TimerReady, tmp.to_js());
                // Box::from_raw(cmd);
              };
            }
            None => println!("no message"),
          };
          println!("sent a message");
          Ok(())
        }),
    ).unwrap(); // TODO: don't unwrap
                // }
}

use std::mem;

// fn handle_decode(_rt: &Runtime, cmd_id: i32, args: Vec<Value>) -> HandlerResult {
//   let arr = match args[0] {
//     Value::Uint8Array(a) => a,
//     _ => panic!("don't panic later."),
//   };

//   match String::from_utf8(unsafe {
//     Vec::from_raw_parts(arr.data_ptr, arr.data_len as usize, arr.data_len)
//   }) {
//     Err(e) => panic!(e),
//     Ok(s) => {
//       let cstr = CString::new(s).unwrap();
//       let strptr = cstr.as_ptr();
//       mem::forget(cstr);
//       Box::new(future::ok(Some(Box::new(Command::new(cmd_id as u32, )) {
//         cmd_id: cmd_id,
//         name: String::from("decode"),
//         args: vec![Value::String(strptr)],
//       })))
//     }
//   }
// }

fn handle_timer_start(rt: &Runtime, id: u32, msg: MessagePayload) -> HandlerResult {
  let ts = match msg {
    MessagePayload::TimerStart(ts) => ts,
    _ => unimplemented!(),
  };
  let timer_id = ts.id;
  let delay = ts.delay;
  // let timer_id = match args[0] {
  //   Value::Int32(i) => i as u32,
  //   _ => panic!("ahhh"),
  // };

  // let delay = match args[1] {
  //   Value::Int32(i) => i as u32,
  //   _ => panic!("ahhh"),
  // };

  let timers = &rt.timers;
  let _el = &rt.rt;
  let ptr = rt.ptr;

  let fut = {
    let (delay_task, cancel_delay) = set_timeout(
      move || {
        remove_timer(ptr, timer_id);
        // send_timer_ready(ptr, timer_id, true);
      },
      delay,
    );

    timers.lock().unwrap().insert(timer_id, cancel_delay);
    delay_task
  };
  // }
  Box::new(fut.then(move |_result| -> OpResult {
    println!("we're ready to notify");
    Ok(Some(Command::new(
      id,
      Message::TimerReady(TimerReady::new(timer_id)),
    )))
  }))
}

fn remove_timer(ptr: JsRuntime, timer_id: u32) {
  let rt = from_c(ptr.0);
  rt.timers.lock().unwrap().remove(&timer_id);
}

fn handle_timer_clear(rt: &Runtime, _cmd_id: u32, args: Vec<Value>) -> HandlerResult {
  let timer_id = match args[0] {
    Value::Int32(i) => i as u32,
    _ => panic!("ahhh"),
  };

  remove_timer(rt.ptr, timer_id);

  Box::new(future::ok(None))
}

// fn handle_http_response(rt: &Runtime, base: msg::Base) -> HandlerResult {
//   println!("handle_http_response");
//   let msg = base.msg_as_http_response().unwrap();
//   let response_id = msg.id();
//   let cmd_id = base.cmd_id();

//   let mut responses = rt.responses.lock().unwrap();
//   let sender = responses.get_mut(&response_id).unwrap();

//   // sender.send(msg);

//   // res.handle(msg);

//   // Ok(null_buf())
//   ok_future(None)
// }

fn set_timeout<F>(cb: F, delay: u32) -> (impl Future<Item = (), Error = ()>, oneshot::Sender<()>)
where
  F: FnOnce() -> (),
{
  let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
  let when = Instant::now() + Duration::from_millis(delay.into());
  let delay_task = Delay::new(when)
    .map_err(|e| panic!("timer failed; err={:?}", e))
    .and_then(|_| {
      cb();
      Ok(())
    }).select(cancel_rx)
    .map(|_| ())
    .map_err(|_| ());

  (delay_task, cancel_tx)
}

// pub fn create_msg(cmd_id: u32, builder: &mut FlatBufferBuilder, mut args: msg::BaseArgs) -> Buf {
//   args.cmd_id = cmd_id;
//   let base = msg::Base::create(builder, &args);
//   msg::finish_base_buffer(builder, base);
//   let data = builder.finished_data();
//   let vec = data.to_vec();
//   Some(vec.into_boxed_slice())
// }

// #[test]
// fn it_tests_right() {
//   let rt = Runtime::new();
//   rt.eval(
//     "test.js",
//     "libfly.log('HELLO FROM JS')
//     libfly.recv((...args)=>{
//       libfly.log(`length: ${args.length}`)
//       libfly.log(`event: ${args[0]} value: ${args[1]} -- ${args[2]}`)
//       libfly.log(`return val: ${libfly.send('a name', 19)}`)
//     })",
//   );
//   let vals: Vec<Value> = vec![Value::Int32(15), Value::Int32(22)];
//   assert!(
//     unsafe {
//       testy(
//         rt.ptr.0,
//         0,
//         CString::new("eventname").unwrap().as_ptr(),
//         vals.len() as libc::c_int,
//         vals.as_ptr(),
//       )
//     } == true
//   );
// }
