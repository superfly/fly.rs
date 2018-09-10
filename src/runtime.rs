extern crate libc;

use tokio;
use tokio::prelude::*;

use std::ffi::CString;
use std::slice;
use std::sync::{Arc, Mutex, Once};

use super::msg;
use std::fs::File;
use std::io::Read;

use js_sys::*;

use futures::sync::oneshot;
use std::collections::HashMap;

use std::thread;
use tokio::runtime::current_thread;

use flatbuffers::FlatBufferBuilder;

use tokio::timer::{Delay, Interval};

use futures::future;
use futures::task;
use std::time::{Duration, Instant};

extern crate hyper;

#[derive(Debug)]
pub struct ResponseFuture {
  response: Option<hyper::Response<hyper::Body>>,
  task: Option<task::Task>,
}

impl ResponseFuture {
  pub fn new() -> Self {
    ResponseFuture {
      response: None,
      task: None,
    }
  }
  pub fn with_response(res: hyper::Response<hyper::Body>) -> Self {
    ResponseFuture {
      response: Some(res),
      task: None,
    }
  }
  fn handle(&mut self, msg: msg::HttpResponse) {
    self.response = Some(hyper::Response::new(hyper::Body::from(
      msg.body().unwrap().to_string(),
    )));
    self.task.take().unwrap().notify();
  }
}

impl Future for ResponseFuture {
  type Item = hyper::Response<hyper::Body>;
  type Error = hyper::Error;

  fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
    match self.response.take() {
      Some(res) => Ok(Async::Ready(res)),
      None => {
        self.task = Some(task::current());
        Ok(Async::NotReady)
      }
    }
  }
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
  pub responses: Mutex<HashMap<u32, ResponseFuture>>,
}

static JSINIT: Once = Once::new();

impl Runtime {
  pub fn new() -> Box<Self> {
    JSINIT.call_once(|| unsafe {
      js_init(
        fly_buf {
          ptr: NATIVES_DATA.as_ptr(),
          len: NATIVES_DATA.len(),
        },
        fly_buf {
          ptr: SNAPSHOT_DATA.as_ptr(),
          len: SNAPSHOT_DATA.len(),
        },
      )
    });

    // let (tx_job, rx_job) = mpsc::channel::<()>(1);
    // let fut = rx_job.for_each(|_f| {
    //   println!("HELLO FROM JOB CHAN");
    //   Ok(())
    // });

    let (c, p) = oneshot::channel::<current_thread::Handle>();
    thread::spawn(move || {
      let mut l = current_thread::Runtime::new().unwrap();
      let task = Interval::new_interval(Duration::from_secs(5))
        .for_each(move |_| {
          // println!("keepalive");
          Ok(())
        })
        .map_err(|e| panic!("interval errored; err={:?}", e));
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

    (*rt_box).ptr.0 = unsafe { js_runtime_new(rt_box.as_ref() as *const _ as *const libc::c_void) };

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

lazy_static! {
  static ref NATIVES_DATA: &'static [u8] =
    include_bytes!("../third_party/v8/out.gn/x64.debug/natives_blob.bin");
  static ref SNAPSHOT_DATA: &'static [u8] =
    include_bytes!("../third_party/v8/out.gn/x64.debug/snapshot_blob.bin");
}

// Buf represents a byte array returned from a "Op".
// The message might be empty (which will be translated into a null object on
// the javascript side) or it is a heap allocated opaque sequence of bytes.
// Usually a flatbuffer message.
type Buf = Option<Box<[u8]>>;

// JS promises in Deno map onto a specific Future
// which yields either a DenoError or a byte array.
type Op = Future<Item = Buf, Error = String> + Send;

type OpResult = Result<Buf, String>;

type HandlerResult = Box<Op>;
type Handler = fn(rt: &Runtime, base: msg::Base) -> HandlerResult;

#[no_mangle]
pub extern "C" fn msg_from_js(raw: *const js_runtime, buf: fly_bytes) {
  println!("got msg from js!");
  let rt = from_c(raw);
  println!("rt: {:?}", rt);
  let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
  let base = msg::get_root_as_base(bytes);
  let msg_type = base.msg_type();
  let cmd_id = base.cmd_id();
  println!("{:?} w/ id: {}", msg_type, cmd_id);

  let handler: Handler = match msg_type {
    msg::Any::TimerStart => handle_timer_start,
    msg::Any::TimerClear => handle_timer_clear,
    msg::Any::HttpResponse => handle_http_response,
    _ => panic!(format!(
      "Unhandled message {}",
      msg::enum_name_any(msg_type)
    )),
  };

  let builder = &mut FlatBufferBuilder::new();
  let fut = handler(rt, base);

  let fut = fut.or_else(move |err| {
    // No matter whether we got an Err or Ok, we want a serialized message to
    // send back. So transform the DenoError into a deno_buf.
    let builder = &mut FlatBufferBuilder::new();
    let errmsg_offset = builder.create_string(&format!("{}", err));
    Ok(create_msg(
      cmd_id,
      builder,
      msg::BaseArgs {
        error: Some(errmsg_offset),
        error_kind: msg::ErrorKind::Other, // err.kind(),
        ..Default::default()
      },
    ))
  });

  if base.sync() {
    // Execute future synchronously.
    // println!("sync handler {}", msg::enum_name_any(msg_type));
    let maybe_box_u8 = fut.wait().unwrap();
    return match maybe_box_u8 {
      None => {}
      Some(box_u8) => {
        let buf = fly_bytes_from(box_u8);
        // Set the synchronous response, the value returned from deno.send().
        unsafe { js_set_response(raw, buf) }
      }
    };
  }

  let ptr = rt.ptr;
  // Execute future asynchornously.
  rt.rt.lock().unwrap().spawn(
    fut
      .map_err(|e: String| println!("ERROR SPAWNING SHIT: {}", e))
      .and_then(move |maybe_box_u8| {
        let buf = match maybe_box_u8 {
          Some(box_u8) => fly_bytes_from(box_u8),
          None => null_buf(),
        };
        // TODO(ry) make this thread safe.
        unsafe { js_send(ptr.0, buf) };
        Ok(())
      }),
  );
}

// TODO(ry) Use Deno instead of DenoC as first arg.
fn remove_timer(ptr: JsRuntime, timer_id: u32) {
  let rt = from_c(ptr.0);
  rt.timers.lock().unwrap().remove(&timer_id);
}

fn handle_http_response(rt: &Runtime, base: msg::Base) -> HandlerResult {
  println!("handle_http_response");
  let msg = base.msg_as_http_response().unwrap();
  let response_id = msg.id();
  let cmd_id = base.cmd_id();

  let mut responses = rt.responses.lock().unwrap();
  let res = responses.get_mut(&response_id).unwrap();

  res.handle(msg);

  // Ok(null_buf())
  ok_future(None)
}

fn fly_bytes_from(x: Box<[u8]>) -> fly_bytes {
  let len = x.len();
  let ptr = Box::into_raw(x);
  fly_bytes {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: ptr as *mut u8,
    data_len: len,
  }
}

// Prototype: https://github.com/ry/deno/blob/golang/timers.go#L25-L39
fn handle_timer_start(rt: &Runtime, base: msg::Base) -> HandlerResult {
  println!("handle_timer_start");
  let msg = base.msg_as_timer_start().unwrap();
  let cmd_id = base.cmd_id();
  let timer_id = msg.id();
  // let interval = msg.interval();
  let delay = msg.delay();

  let timers = &rt.timers;
  let el = &rt.rt;
  let ptr = rt.ptr;

  // if interval {
  //   let (interval_task, cancel_interval) = set_interval(
  //     move || {
  //       send_timer_ready(ptr, timer_id, false);
  //     },
  //     delay,
  //   );

  //   timers.lock().unwrap().insert(timer_id, cancel_interval);
  //   el.lock().unwrap().spawn(interval_task);
  // } else {
  let fut = {
    let (delay_task, cancel_delay) = set_timeout(
      move || {
        remove_timer(ptr, timer_id);
        // send_timer_ready(ptr, timer_id, true);
      },
      delay,
    );

    timers.lock().unwrap().insert(timer_id, cancel_delay);
    // el.lock().unwrap().spawn(delay_task);
    delay_task
  };
  // }
  Box::new(fut.then(move |result| {
    let builder = &mut FlatBufferBuilder::new();
    let msg = msg::TimerReady::create(
      builder,
      &msg::TimerReadyArgs {
        id: timer_id,
        canceled: result.is_err(),
        ..Default::default()
      },
    );
    Ok(create_msg(
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

// Prototype: https://github.com/ry/deno/blob/golang/timers.go#L40-L43
fn handle_timer_clear(rt: &Runtime, base: msg::Base) -> HandlerResult {
  let msg = base.msg_as_timer_clear().unwrap();
  println!("handle_timer_clear");
  remove_timer(rt.ptr, msg.id());
  ok_future(None)
}

fn ok_future(buf: Buf) -> Box<Op> {
  Box::new(future::ok(buf))
}

// Shout out to Earl Sweatshirt.
fn odd_future(err: String) -> Box<Op> {
  Box::new(future::err(err))
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
    })
    .select(cancel_rx)
    .map(|_| ())
    .map_err(|_| ());

  (delay_task, cancel_tx)
}

fn set_interval<F>(cb: F, delay: u32) -> (impl Future<Item = (), Error = ()>, oneshot::Sender<()>)
where
  F: Fn() -> (),
{
  let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
  let delay = Duration::from_millis(delay.into());
  let interval_task = future::lazy(move || {
    Interval::new(Instant::now() + delay, delay)
      .for_each(move |_| {
        cb();
        future::ok(())
      })
      .into_future()
      .map_err(|_| panic!())
  }).select(cancel_rx)
    .map(|_| ())
    .map_err(|_| ());

  (interval_task, cancel_tx)
}

fn null_buf() -> fly_bytes {
  fly_bytes {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: 0 as *mut u8,
    data_len: 0,
  }
}

pub fn create_msg(cmd_id: u32, builder: &mut FlatBufferBuilder, mut args: msg::BaseArgs) -> Buf {
  args.cmd_id = cmd_id;
  let base = msg::Base::create(builder, &args);
  msg::finish_base_buffer(builder, base);
  let data = builder.finished_data();
  // fly_bytes {
  //   // TODO(ry)
  //   // The deno_buf / ImportBuf / ExportBuf semantics should be such that we do not need to yield
  //   // ownership. Temporarally there is a hack in ImportBuf that when alloc_ptr is null, it will
  //   // memcpy the deno_buf into V8 instead of doing zero copy.
  //   alloc_ptr: 0 as *mut u8,
  //   alloc_len: 0,
  //   data_ptr: data.as_ptr() as *mut u8,
  //   data_len: data.len(),
  // }
  let vec = data.to_vec();
  Some(vec.into_boxed_slice())
}
