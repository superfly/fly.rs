extern crate libc;
// extern crate tokio;

use tokio;
use tokio::prelude::*;

use std::ffi::CString;
use std::slice;
use std::sync::{Mutex, Once};

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

use std::time::{Duration, Instant};

use futures::sync::mpsc;

#[derive(Debug, Copy, Clone)]
struct JSRuntime(pub *const js_runtime);
unsafe impl Send for JSRuntime {}
unsafe impl Sync for JSRuntime {}

#[derive(Debug)]
pub struct Runtime {
  ptr: JSRuntime,
  rt: Mutex<tokio::runtime::current_thread::Handle>,
  timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
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
          println!("keepalive");
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
      ptr: JSRuntime(0 as *const js_runtime),
      rt: Mutex::new(p.wait().unwrap()),
      timers: Mutex::new(HashMap::new()),
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

type HandlerResult = Result<fly_bytes, String>;
type Handler = fn(rt: &Runtime, base: msg::Base, builder: &mut FlatBufferBuilder) -> HandlerResult;

#[no_mangle]
pub extern "C" fn msg_from_js(raw: *const js_runtime, buf: fly_bytes) {
  println!("got msg from js!");
  let rt = from_c(raw);
  println!("rt: {:?}", rt);
  let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
  let base = msg::get_root_as_base(bytes);
  let msg_type = base.msg_type();
  println!("{:?}", msg_type);

  let handler: Handler = match msg_type {
    msg::Any::TimerStart => handle_timer_start,
    msg::Any::TimerClear => handle_timer_clear,
    _ => panic!(format!(
      "Unhandled message {}",
      msg::enum_name_any(msg_type)
    )),
  };

  let builder = &mut FlatBufferBuilder::new();
  let result = handler(rt, base, builder);

  // No matter whether we got an Err or Ok, we want a serialized message to
  // send back. So transform the DenoError into a deno_buf.
  let buf = match result {
    Err(ref err) => {
      let errmsg_offset = builder.create_string(&format!("{}", err));
      create_msg(
        builder,
        &msg::BaseArgs {
          error: Some(errmsg_offset),
          error_kind: msg::ErrorKind::Other, // err.kind(),
          ..Default::default()
        },
      )
    }
    Ok(buf) => buf,
  };

  // Set the synchronous response, the value returned from deno.send().
  // null_buf is a special case that indicates success.
  if buf != null_buf() {
    unsafe { js_set_response(rt.ptr.0, buf) }
  }
}

// TODO(ry) Use Deno instead of DenoC as first arg.
fn remove_timer(ptr: JSRuntime, timer_id: u32) {
  let rt = from_c(ptr.0);
  rt.timers.lock().unwrap().remove(&timer_id);
}

// Prototype: https://github.com/ry/deno/blob/golang/timers.go#L25-L39
fn handle_timer_start(
  rt: &Runtime,
  base: msg::Base,
  _builder: &mut FlatBufferBuilder,
) -> HandlerResult {
  println!("handle_timer_start");
  let msg = base.msg_as_timer_start().unwrap();
  let timer_id = msg.id();
  let interval = msg.interval();
  let delay = msg.delay();

  let timers = &rt.timers;
  let el = &rt.rt;
  let ptr = rt.ptr;

  if interval {
    let (interval_task, cancel_interval) = set_interval(
      move || {
        send_timer_ready(ptr, timer_id, false);
      },
      delay,
    );

    timers.lock().unwrap().insert(timer_id, cancel_interval);
    el.lock().unwrap().spawn(interval_task);
  } else {
    let (delay_task, cancel_delay) = set_timeout(
      move || {
        remove_timer(ptr, timer_id);
        send_timer_ready(ptr, timer_id, true);
      },
      delay,
    );

    timers.lock().unwrap().insert(timer_id, cancel_delay);
    el.lock().unwrap().spawn(delay_task);
  }
  Ok(null_buf())
}

// Prototype: https://github.com/ry/deno/blob/golang/timers.go#L40-L43
fn handle_timer_clear(
  rt: &Runtime,
  base: msg::Base,
  _builder: &mut FlatBufferBuilder,
) -> HandlerResult {
  let msg = base.msg_as_timer_clear().unwrap();
  println!("handle_timer_clear");
  remove_timer(rt.ptr, msg.id());
  Ok(null_buf())
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

// TODO(ry) Use Deno instead of DenoC as first arg.
fn send_timer_ready(ptr: JSRuntime, timer_id: u32, done: bool) {
  let mut builder = FlatBufferBuilder::new();
  let msg = msg::TimerReady::create(
    &mut builder,
    &msg::TimerReadyArgs {
      id: timer_id,
      done,
      ..Default::default()
    },
  );
  send_base(
    ptr.0,
    &mut builder,
    &msg::BaseArgs {
      msg: Some(msg.as_union_value()),
      msg_type: msg::Any::TimerReady,
      ..Default::default()
    },
  );
}

fn null_buf() -> fly_bytes {
  fly_bytes {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: 0 as *mut u8,
    data_len: 0,
  }
}

fn send_base(ptr: *const js_runtime, builder: &mut FlatBufferBuilder, args: &msg::BaseArgs) -> i32 {
  let buf = create_msg(builder, args);
  unsafe { js_send(ptr, buf) }
}

fn create_msg(builder: &mut FlatBufferBuilder, args: &msg::BaseArgs) -> fly_bytes {
  let base = msg::Base::create(builder, &args);
  msg::finish_base_buffer(builder, base);
  let data = builder.finished_data();
  fly_bytes {
    // TODO(ry)
    // The deno_buf / ImportBuf / ExportBuf semantics should be such that we do not need to yield
    // ownership. Temporarally there is a hack in ImportBuf that when alloc_ptr is null, it will
    // memcpy the deno_buf into V8 instead of doing zero copy.
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: data.as_ptr() as *mut u8,
    data_len: data.len(),
  }
}
