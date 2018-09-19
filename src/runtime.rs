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

extern crate libfly;

extern crate hyper;

use flatbuffers::FlatBufferBuilder;
use msg;

#[derive(Debug)]
pub struct JsHttpResponse {
  pub headers: Mutex<HashMap<String, String>>,
}

#[derive(Debug, Copy, Clone)]
pub struct JsRuntime(pub *const js_runtime);
unsafe impl Send for JsRuntime {}
unsafe impl Sync for JsRuntime {}

#[derive(Debug)]
pub struct Runtime {
  pub ptr: JsRuntime,
  pub rt: Mutex<tokio::runtime::current_thread::Handle>,
  timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
  pub responses: Mutex<HashMap<u32, oneshot::Sender<JsHttpResponse>>>,
}

static JSINIT: Once = Once::new();

impl Runtime {
  pub fn new() -> Box<Self> {
    JSINIT.call_once(|| unsafe {
      js_init(
        fly_simple_buf {
          ptr: NATIVES_DATA.as_ptr() as *const i8,
          len: NATIVES_DATA.len() as i32,
        },
        fly_simple_buf {
          ptr: SNAPSHOT_DATA.as_ptr() as *const i8,
          len: SNAPSHOT_DATA.len() as i32,
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
  static ref FLY_SNAPSHOT: fly_simple_buf = unsafe {
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
pub type Buf = Option<Box<[u8]>>;

// JS promises in Deno map onto a specific Future
// which yields either a DenoError or a byte array.
type Op = Future<Item = Buf, Error = String> + Send;

type OpResult = Result<Buf, String>;

type Handler = fn(rt: &Runtime, base: &msg::Base) -> Box<Op>;

use std::slice;

#[no_mangle]
pub extern "C" fn msg_from_js(raw: *const js_runtime, buf: fly_buf) {
  let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
  let base = msg::get_root_as_base(bytes);
  let msg_type = base.msg_type();
  let cmd_id = base.cmd_id();
  // println!("msg id {}", cmd_id);
  let handler: Handler = match msg_type {
    msg::Any::TimerStart => handle_timer_start,
    msg::Any::TimerClear => handle_timer_clear,
    msg::Any::HttpResponse => handle_http_response,
    _ => unimplemented!(),
  };

  let rt = from_c(raw);
  let ptr = rt.ptr;

  let fut = handler(rt, &base);
  let fut = fut.or_else(move |err| {
    // No matter whether we got an Err or Ok, we want a serialized message to
    // send back. So transform the DenoError into a deno_buf.
    let builder = &mut FlatBufferBuilder::new();
    let errmsg_offset = builder.create_string(&format!("{}", err));
    Ok(serialize_response(
      cmd_id,
      builder,
      msg::BaseArgs {
        error: Some(errmsg_offset),
        error_kind: msg::ErrorKind::NoError, // err.kind
        ..Default::default()
      },
    ))
  });

  if base.sync() {
    // Execute future synchronously.
    let maybe_box_u8 = fut.wait().unwrap();
    match maybe_box_u8 {
      None => {}
      Some(box_u8) => {
        let buf = fly_buf_from(box_u8);
        // Set the synchronous response, the value returned from deno.send().
        unsafe { js_set_response(ptr.0, buf) }
      }
    }
  } else {
    let fut = fut.and_then(move |maybe_box_u8| {
      let buf = match maybe_box_u8 {
        Some(box_u8) => fly_buf_from(box_u8),
        None => {
          // async RPCs that return None still need to
          // send a message back to signal completion.
          let builder = &mut FlatBufferBuilder::new();
          fly_buf_from(
            serialize_response(
              cmd_id,
              builder,
              msg::BaseArgs {
                ..Default::default()
              },
            ).unwrap(),
          )
        }
      };
      unsafe { js_send(ptr.0, buf) };
      Ok(())
    });
    rt.rt.lock().unwrap().spawn(fut);
  }
}

fn ok_future(buf: Buf) -> Box<Op> {
  Box::new(future::ok(buf))
}

// Shout out to Earl Sweatshirt.
fn odd_future(err: String) -> Box<Op> {
  Box::new(future::err(err))
}

pub fn fly_buf_from(x: Box<[u8]>) -> fly_buf {
  let len = x.len();
  let ptr = Box::into_raw(x);
  fly_buf {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: ptr as *mut u8,
    data_len: len,
  }
}

use std::mem;

fn handle_timer_start(rt: &Runtime, base: &msg::Base) -> Box<Op> {
  println!("handle_timer_start");
  let msg = base.msg_as_timer_start().unwrap();
  let cmd_id = base.cmd_id();
  let timer_id = msg.id();
  let delay = msg.delay();

  let timers = &rt.timers;
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
  Box::new(fut.then(move |result| -> OpResult {
    // println!("we're ready to notify");
    let builder = &mut FlatBufferBuilder::new();
    let msg = msg::TimerReady::create(
      builder,
      &msg::TimerReadyArgs {
        id: timer_id,
        canceled: result.is_err(),
        ..Default::default()
      },
    );
    Ok(serialize_response(
      cmd_id,
      builder,
      msg::BaseArgs {
        msg: Some(msg.as_union_value()),
        msg_type: msg::Any::TimerReady,
        ..Default::default()
      },
    ))
  }))
}

pub fn serialize_response(
  cmd_id: u32,
  builder: &mut FlatBufferBuilder,
  mut args: msg::BaseArgs,
) -> Buf {
  args.cmd_id = cmd_id;
  let base = msg::Base::create(builder, &args);
  msg::finish_base_buffer(builder, base);
  let data = builder.finished_data();
  // println!("serialize_response {:x?}", data);
  let vec = data.to_vec();
  Some(vec.into_boxed_slice())
}

fn remove_timer(ptr: JsRuntime, timer_id: u32) {
  let rt = from_c(ptr.0);
  rt.timers.lock().unwrap().remove(&timer_id);
}

fn handle_timer_clear(rt: &Runtime, base: &msg::Base) -> Box<Op> {
  let msg = base.msg_as_timer_clear().unwrap();
  println!("handle_timer_clear");
  remove_timer(rt.ptr, msg.id());
  ok_future(None)
}

fn handle_http_response(rt: &Runtime, base: &msg::Base) -> Box<Op> {
  // println!("handle_http_response");
  let msg = base.msg_as_http_response().unwrap();
  let req_id = msg.id();

  let msg_headers = msg.headers().unwrap();

  let mut headers: HashMap<String, String> = HashMap::new();
  for i in 0..msg_headers.len() {
    let h = msg_headers.get(i);
    headers.insert(h.key().unwrap().to_string(), h.value().unwrap().to_string());
  }

  let mut responses = rt.responses.lock().unwrap();
  match responses.remove(&req_id) {
    Some(mut sender) => {
      sender.send(JsHttpResponse {
        headers: Mutex::new(headers),
      });
    }
    _ => unimplemented!(),
  };

  ok_future(None)
}

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
