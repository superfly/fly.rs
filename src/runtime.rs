extern crate libc;

use tokio;
use tokio::prelude::*;

use std::io;

use std::ffi::CString;
use std::sync::{Arc, Mutex, Once, RwLock};

use std::fs::File;
use std::io::Read;

use libfly::*;

use std::sync::mpsc as stdmspc;

use futures::sync::{mpsc, oneshot};
use std::collections::HashMap;

use std::thread;
use tokio::runtime::current_thread;

use tokio::timer::{Delay, Interval};

use std::time::{Duration, Instant};

use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

use futures::future;

extern crate hyper;
extern crate r2d2_redis;
use self::r2d2_redis::{r2d2, redis, RedisConnectionManager};
use self::redis::Commands;

use self::hyper::body::Payload;
use self::hyper::client::HttpConnector;
use self::hyper::header::HeaderName;
use self::hyper::rt::{poll_fn, Future, Stream};
use self::hyper::HeaderMap;
use self::hyper::{Body, Client, Method, Request, Response, StatusCode};

use flatbuffers::FlatBufferBuilder;
use msg;

use errors::{FlyError, FlyResult};

use redis_stream;

#[derive(Debug)]
pub struct JsHttpResponse {
  pub headers: HeaderMap,
  pub bytes: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
}

// #[derive(Debug)]
// pub struct JsHttpRequest {
//   pub headers: HeaderMap,
//   pub method: Method,
//   pub url: String,
//   pub bytes: Option<mpsc::Receiver<Chunk>>,
// }

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
  // pub bytes_recv: Mutex<HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>>,
  pub bytes: Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>,
  pub http_client: Client<HttpConnector, Body>,
}

static JSINIT: Once = Once::new();

use std::ptr as stdptr;

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
      bytes: Mutex::new(HashMap::new()),
      http_client: Client::new(), //Client::builder().set_host(false).build_http(),
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

  pub fn heap_statistics(&self) -> js_heap_stats {
    unsafe { js_runtime_heap_statistics(self.ptr.0) }
  }
}

pub fn from_c<'a>(rt: *const js_runtime) -> &'a mut Runtime {
  let ptr = unsafe { js_get_data(rt) };
  let rt_ptr = ptr as *mut Runtime;
  let rt_box = unsafe { Box::from_raw(rt_ptr) };
  Box::leak(rt_box)
}

const NATIVES_DATA: &'static [u8] =
  include_bytes!("../third_party/v8/out.gn/x64.debug/natives_blob.bin");
const SNAPSHOT_DATA: &'static [u8] =
  include_bytes!("../third_party/v8/out.gn/x64.debug/snapshot_blob.bin");
const V8ENV_SOURCEMAP: &'static [u8] = include_bytes!("../fly/packages/v8env/dist/v8env.js.map");

extern crate tokio_io_pool;

// pub static mut EVENT_LOOP: Option<tokio_io_pool::Runtime> = None;

extern crate sourcemap;
// use self::sourcemap;
use self::sourcemap::{DecodedMap, SourceMap};
use std::fs;

lazy_static! {
  static ref FLY_SNAPSHOT: fly_simple_buf = unsafe {

    let filename = "fly/packages/v8env/dist/v8env.js";
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    js_create_snapshot(
      CString::new("v8env.js").unwrap().as_ptr(),
      CString::new(contents).unwrap().as_ptr(),
    )
  };

  static ref SM_CHAN: Mutex<stdmspc::Sender<(Vec<(u32, u32, String,String)>, oneshot::Sender<Vec<(u32, u32, String, String)>>)>> = {
    let (sender, receiver) = stdmspc::channel::<(Vec<(u32, u32, String, String)>, oneshot::Sender<Vec<(u32, u32, String, String)>>)>();
    thread::spawn(move||{
      let sm = SourceMap::from_reader(V8ENV_SOURCEMAP).unwrap();
      for tup in receiver.iter() {
        let ch = tup.1;
        let v = tup.0;
        ch.send(v.iter()
          .map(|(line, col, name, filename)| {
            if filename == "v8env.js" {
              return match sm.lookup_token(*line, *col) {
                Some(t) => {
                  let newline = t.get_src_line();
                  let newcol = t.get_src_col();
                  let newfilename = match t.get_source() {
                    Some(s) => String::from(s),
                    None => filename.clone()
                  };
                  (newline, newcol, name.clone(), newfilename)
                }
                None => (*line, *col, name.clone(), filename.clone())
              };
            }
            (*line, *col, name.clone(), filename.clone())
          }).collect()
        );
      }
    });
    Mutex::new(sender)
  };

  // static ref V8ENV_SOURCEMAP: Arc<Mutex<SourceMap>> = {
  //   let mut f = fs::File::open("fly/packages/v8env/dist/v8env.js.map").unwrap();
  //   match sourcemap::decode(&mut f).unwrap() {
  //     DecodedMap::Regular(s) => Arc::new(Mutex::new(s)),
  //     _ => unimplemented!()
  //   }
  // };
  // pub static ref EVENT_LOOP: Arc<tokio_io_pool::Runtime> = Arc::new(tokio_io_pool::Runtime::new());
}

// pub static mut EVENT_LOOP: Option<Mutex<tokio_io_pool::Runtime>> = None;
pub static mut EVENT_LOOP_HANDLE: Option<Arc<Mutex<tokio_io_pool::Handle>>> = None;

// Buf represents a byte array returned from a "Op".
// The message might be empty (which will be translated into a null object on
// the javascript side) or it is a heap allocated opaque sequence of bytes.
// Usually a flatbuffer message.
pub type Buf = Option<Box<[u8]>>;

// JS promises in Deno map onto a specific Future
// which yields either a DenoError or a byte array.
type Op = Future<Item = Buf, Error = FlyError> + Send;

type OpResult = FlyResult<Buf>;

type Handler = fn(rt: &Runtime, base: &msg::Base, raw_buf: fly_buf) -> Box<Op>;

use std::slice;

#[no_mangle]
pub extern "C" fn msg_from_js(raw: *const js_runtime, buf: fly_buf, raw_buf: fly_buf) {
  let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
  let base = msg::get_root_as_base(bytes);
  let msg_type = base.msg_type();
  // println!("MSG TYPE: {:?}", msg_type);
  let cmd_id = base.cmd_id();
  // println!("msg id {}", cmd_id);
  let handler: Handler = match msg_type {
    msg::Any::TimerStart => handle_timer_start,
    msg::Any::TimerClear => handle_timer_clear,
    msg::Any::HttpRequest => handle_http_request,
    msg::Any::HttpResponse => handle_http_response,
    msg::Any::StreamChunk => handle_stream_chunk,
    msg::Any::CacheGet => handle_cache_get,
    msg::Any::CacheSet => handle_cache_set,
    msg::Any::CryptoDigest => handle_crypto_digest,
    msg::Any::SourceMap => handle_source_map,
    _ => unimplemented!(),
  };

  let rt = from_c(raw);
  let ptr = rt.ptr;

  let fut = handler(rt, &base, raw_buf);
  let fut = fut.or_else(move |err| {
    println!("OR ELSE, we got an error man... {:?}", err);
    // No matter whether we got an Err or Ok, we want a serialized message to
    // send back. So transform the DenoError into a deno_buf.
    let builder = &mut FlatBufferBuilder::new();
    let errmsg_offset = builder.create_string(&format!("{}", err));
    Ok(serialize_response(
      cmd_id,
      builder,
      msg::BaseArgs {
        error: Some(errmsg_offset),
        error_kind: err.kind(), // err.kind
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
      unsafe { js_send(ptr.0, buf, null_buf()) };
      Ok(())
    });
    rt.rt.lock().unwrap().spawn(fut);
  }
}

pub fn null_buf() -> fly_buf {
  fly_buf {
    alloc_ptr: 0 as *mut u8,
    alloc_len: 0,
    data_ptr: 0 as *mut u8,
    data_len: 0,
  }
}

fn ok_future(buf: Buf) -> Box<Op> {
  Box::new(future::ok(buf))
}

// Shout out to Earl Sweatshirt.
fn odd_future(err: FlyError) -> Box<Op> {
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

fn handle_timer_start(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
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
  Box::new(fut.then(move |result| {
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

fn handle_timer_clear(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_timer_clear().unwrap();
  println!("handle_timer_clear");
  remove_timer(rt.ptr, msg.id());
  ok_future(None)
}

fn handle_source_map(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_source_map().unwrap();

  let msg_frames = msg.frames().unwrap();
  let mut frames = Vec::with_capacity(msg_frames.len());

  for i in 0..msg_frames.len() {
    let f = msg_frames.get(i);

    println!(
      "got frame: {:?} {:?} {:?} {:?}",
      f.name(),
      f.filename(),
      f.line(),
      f.col()
    );

    let name = match f.name() {
      Some(n) => n,
      None => "",
    };

    let mut filename = match f.filename() {
      Some(f) => f,
      None => "",
    };

    let mut line = f.line();
    let mut col = f.col();

    frames.insert(i, (line, col, String::from(name), String::from(filename)));
  }

  let (tx, rx) = oneshot::channel::<Vec<(u32, u32, String, String)>>();
  SM_CHAN.lock().unwrap().send((frames, tx));

  Box::new(
    rx.map_err(|e| FlyError::from(format!("{}", e)))
      .and_then(move |v| {
        let builder = &mut FlatBufferBuilder::new();
        let framed: Vec<_> = v
          .iter()
          .map(|(line, col, name, filename)| {
            let namefbb = builder.create_string(name.as_str());
            let filenamefbb = builder.create_string(filename.as_str());
            msg::Frame::create(
              builder,
              &msg::FrameArgs {
                name: Some(namefbb),
                filename: Some(filenamefbb),
                line: *line,
                col: *col,
              },
            )
          }).collect();
        let ret_frames = builder.create_vector(&framed);

        let ret_msg = msg::SourceMapReady::create(
          builder,
          &msg::SourceMapReadyArgs {
            frames: Some(ret_frames),
            ..Default::default()
          },
        );
        Ok(serialize_response(
          cmd_id,
          builder,
          msg::BaseArgs {
            msg: Some(ret_msg.as_union_value()),
            msg_type: msg::Any::SourceMapReady,
            ..Default::default()
          },
        ))
      }),
  )
}
// match rx.wait() {
//   Ok(v) => {
//     println!("got a vec! {:?}", v);
//   }
//   Err(e) => {
//     return
//     println!("ERROR USING SM CHAN {}", e);
//   }
// };

// if filename == "v8env.js" {
//   match sm.lookup_token(line, col) {
//     Some(t) => {
//       line = t.get_src_line();
//       col = t.get_src_col();
//       match t.get_source() {
//         Some(s) => filename = s,
//         None => {}
//       };
//     }
//     None => {}
//   };
// }

//     frames.insert(
//       i,
//       msg::Frame::create(
//         builder,
//         &msg::FrameArgs {
//           name: Some(namefbb),
//           filename: Some(filenamefbb),
//           line: line,
//           col: col,
//         },
//       ),
//     );
//   }
// }

// let ret_frames = builder.create_vector(&frames);

// let ret_msg = msg::SourceMapReady::create(
//   builder,
//   &msg::SourceMapReadyArgs {
//     frames: Some(ret_frames),
//     ..Default::default()
//   },
// );
// ok_future(serialize_response(
//   cmd_id,
//   builder,
//   msg::BaseArgs {
//     msg: Some(ret_msg.as_union_value()),
//     msg_type: msg::Any::SourceMapReady,
//     ..Default::default()
//   },
// ))
// }

extern crate sha1;
extern crate sha2; // SHA-1 // SHA-256, etc.
use self::sha1::Digest as Sha1Digest;
use self::sha1::Sha1;
use self::sha2::Digest;
use self::sha2::Sha256;

fn handle_crypto_digest(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_crypto_digest().unwrap();

  let algo = msg.algo().unwrap().to_uppercase();
  let buffer = unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec();

  Box::new(future::lazy(move || {
    let builder = &mut FlatBufferBuilder::new();
    let bytes_vec = match algo.as_str() {
      "SHA-256" => {
        let mut h = Sha256::default();
        h.input(buffer.as_slice());
        let res = h.result();
        builder.create_vector(res.as_slice())
      }
      "SHA-1" => {
        let mut h = Sha1::default();
        h.input(buffer.as_slice());
        let res = h.result();
        builder.create_vector(res.as_slice())
      }
      _ => unimplemented!(),
    };
    // hasher.input(buffer.as_slice());
    // let res = hasher.result();

    // let bytes_vec = builder.create_vector(res.as_slice());
    let crypto_ready = msg::CryptoDigestReady::create(
      builder,
      &msg::CryptoDigestReadyArgs {
          buffer: Some(bytes_vec),
          ..Default::default()
          // done: body.is_end_stream(),
        },
    );
    Ok(serialize_response(
      cmd_id,
      builder,
      msg::BaseArgs {
        msg: Some(crypto_ready.as_union_value()),
        msg_type: msg::Any::CryptoDigestReady,
        ..Default::default()
      },
    ))
  }))
}

use super::NEXT_STREAM_ID;
use std::str;

use std::ops::Deref;
fn handle_cache_set(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  println!("CACHE SET");
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_cache_set().unwrap();
  let key = msg.key().unwrap().to_string();

  let stream_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let rtptr = rt.ptr;

  let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
  // {
  //   rt.bytes_recv.lock().unwrap().insert(stream_id, recver);
  // }
  {
    rt.bytes.lock().unwrap().insert(stream_id, sender);
  }

  {
    let pool = Arc::clone(&redis_stream::REDIS_CACHE_POOL);
    let con = pool.get().unwrap(); // TODO: no unwrap
    let offset: AtomicUsize = ATOMIC_USIZE_INIT;
    rt.rt.lock().unwrap().spawn(
      recver
        // .into_future()
        .map_err(|_| println!("error cache set stream!"))
        .for_each(move |b| {
          let start = offset.fetch_add(b.len(), Ordering::SeqCst);
          match redis::cmd("SETRANGE").arg(key.clone()).arg(start).arg(b).query::<usize>(con.deref())
          {
            Ok(r) => {}
            Err(e) => println!("error in redis.. {}", e),
          }
          Ok(())
        }),
    );
  }

  let builder = &mut FlatBufferBuilder::new();
  let msg = msg::CacheSetReady::create(
    builder,
    &msg::CacheSetReadyArgs {
      id: stream_id,
      ..Default::default()
    },
  );
  ok_future(serialize_response(
    cmd_id,
    builder,
    msg::BaseArgs {
      msg: Some(msg.as_union_value()),
      msg_type: msg::Any::CacheSetReady,
      ..Default::default()
    },
  ))
}

fn handle_cache_get(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  println!("CACHE GET");
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_cache_get().unwrap();

  let stream_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let rtptr = rt.ptr;

  let key = msg.key().unwrap().to_string();

  let got = {
    let pool = Arc::clone(&redis_stream::REDIS_CACHE_POOL);
    let con = pool.get().unwrap(); // TODO: no unwrap
    match redis::cmd("EXISTS")
      .arg(key.clone())
      .query::<bool>(con.deref())
    {
      Ok(b) => b,
      Err(e) => {
        println!("redis exists err: {}", e);
        false
      }
    }
  };

  {
    // need to hijack the order here.
    rt.rt.lock().unwrap().spawn(future::lazy(move || {
      let builder = &mut FlatBufferBuilder::new();
      let msg = msg::CacheGetReady::create(
        builder,
        &msg::CacheGetReadyArgs {
          id: stream_id,
          stream: got,
          ..Default::default()
        },
      );
      unsafe {
        js_send(
          rtptr.0,
          fly_buf_from(
            serialize_response(
              cmd_id,
              builder,
              msg::BaseArgs {
                msg: Some(msg.as_union_value()),
                msg_type: msg::Any::CacheGetReady,
                ..Default::default()
              },
            ).unwrap(),
          ),
          null_buf(),
        )
      };
      Ok(())
    }));
  }

  if got {
    let stream = redis_stream::redis_stream(key.clone());

    rt.rt.lock().unwrap().spawn(
      stream
      // .into_future()
      .map_err(|e| println!("error redis stream: {}", e))
      .for_each(move |bytes| {
        let builder = &mut FlatBufferBuilder::new();
        let chunk_msg = msg::StreamChunk::create(
          builder,
          &msg::StreamChunkArgs {
            id: stream_id,
            done: false,
            ..Default::default()
          },
        );
        unsafe {
          js_send(
            rtptr.0,
            fly_buf_from(
              serialize_response(
                0,
                builder,
                msg::BaseArgs {
                  msg: Some(chunk_msg.as_union_value()),
                  msg_type: msg::Any::StreamChunk,
                  ..Default::default()
                },
              ).unwrap(),
            ),
            fly_buf {
              alloc_ptr: 0 as *mut u8,
              alloc_len: 0,
              data_ptr: (*bytes).as_ptr() as *mut u8,
              data_len: bytes.len(),
            },
          )
        };
        Ok(())
      }).and_then(move |_| {
        println!("done getting bytes");
        let builder = &mut FlatBufferBuilder::new();
        let chunk_msg = msg::StreamChunk::create(
          builder,
          &msg::StreamChunkArgs {
            id: stream_id,
            done: true,
            ..Default::default()
          },
        );
        unsafe {
          js_send(
            rtptr.0,
            fly_buf_from(
              serialize_response(
                0,
                builder,
                msg::BaseArgs {
                  msg: Some(chunk_msg.as_union_value()),
                  msg_type: msg::Any::StreamChunk,
                  ..Default::default()
                },
              ).unwrap(),
            ),
            null_buf(),
          )
        };
        Ok(())
      }),
    );
  }

  ok_future(None)

  // let builder = &mut FlatBufferBuilder::new();
  // let msg = msg::CacheGetReady::create(
  //   builder,
  //   &msg::CacheGetReadyArgs {
  //     id: stream_id,
  //     stream: got,
  //     ..Default::default()
  //   },
  // );
  // ok_future(serialize_response(
  //   cmd_id,
  //   builder,
  //   msg::BaseArgs {
  //     msg: Some(msg.as_union_value()),
  //     msg_type: msg::Any::CacheGetReady,
  //     ..Default::default()
  //   },
  // ))
}

fn handle_http_request(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_http_request().unwrap();
  let req_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst) as u32;
  let rtptr = rt.ptr;

  let mut req_body: Body;
  if msg.body() {
    unimplemented!();
  } else {
    req_body = Body::empty();
  }

  let mut req = Request::new(req_body);
  {
    let uri: hyper::Uri = msg.url().unwrap().parse().unwrap();
    // println!("url: {:?}", uri);
    *req.uri_mut() = uri;
    *req.method_mut() = match msg.method() {
      msg::HttpMethod::Get => Method::GET,
      msg::HttpMethod::Post => Method::POST,
      _ => unimplemented!(),
    };

    let msg_headers = msg.headers().unwrap();
    let mut headers = req.headers_mut();
    for i in 0..msg_headers.len() {
      let h = msg_headers.get(i);
      // println!("header: {} => {}", h.key().unwrap(), h.value().unwrap());
      headers.insert(
        HeaderName::from_bytes(h.key().unwrap().as_bytes()).unwrap(),
        h.value().unwrap().parse().unwrap(),
      );
    }
  }

  let rtptr2 = rtptr.clone();

  let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

  let fut = rt
    .http_client
    .request(req)
    // .map_err(move |e| {
    //   perr.send(Err(e.into()));
    // })
    .then(move |reserr| {
      if let Err(err) = reserr {
        p.send(Err(err.into()));
        return Ok(())
      }

      let res = reserr.unwrap(); // should be safe.

      let (parts, mut body) = res.into_parts();

      let mut bytes_rx: Option<mpsc::UnboundedReceiver<Vec<u8>>> = None;
      if !body.is_end_stream() {
        let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
        bytes_rx = Some(rx);
        let mut bytes = from_c(rtptr2.0).bytes.lock().unwrap();
        bytes.insert(req_id, tx);
      }

      p.send(Ok(JsHttpResponse {
        headers: parts.headers,
        bytes: bytes_rx,
      }));

      if !body.is_end_stream() {
        let rt = from_c(rtptr.0); // like a clone
        rt.rt.lock().unwrap().spawn(
          poll_fn(move || {
            while let Some(chunk) = try_ready!(body.poll_data()) {
              let mut bytes = chunk.into_bytes();
              let builder = &mut FlatBufferBuilder::new();
              let chunk_msg = msg::StreamChunk::create(
                builder,
                &msg::StreamChunkArgs {
                  id: req_id,
                  done: body.is_end_stream(),
                },
              );
              unsafe {
                js_send(
                  rtptr.0,
                  fly_buf_from(
                    serialize_response(
                      0,
                      builder,
                      msg::BaseArgs {
                        msg: Some(chunk_msg.as_union_value()),
                        msg_type: msg::Any::StreamChunk,
                        ..Default::default()
                      },
                    ).unwrap(),
                  ),
                  fly_buf {
                    alloc_ptr: 0 as *mut u8,
                    alloc_len: 0,
                    data_ptr: (*bytes).as_ptr() as *mut u8,
                    data_len: bytes.len(),
                  },
                )
              };
            }
            Ok(Async::Ready(()))
          }).map_err(|e: hyper::Error| ()),
        );
      }
      Ok(())
    });

  let fut2 = c
    .map_err(|e| {
      FlyError::from(io::Error::new(
        io::ErrorKind::Other,
        format!("err getting response from oneshot: {}", e).as_str(),
      ))
    }).and_then(move |reserr: FlyResult<JsHttpResponse>| {
      if let Err(err) = reserr {
        return Err(err);
      }

      let res = reserr.unwrap();

      let builder = &mut FlatBufferBuilder::new();
      let headers: Vec<_> = res
        .headers
        .iter()
        .map(|(key, value)| {
          let key = builder.create_string(key.as_str());
          let value = builder.create_string(value.to_str().unwrap());
          msg::HttpHeader::create(
            builder,
            &msg::HttpHeaderArgs {
              key: Some(key),
              value: Some(value),
              ..Default::default()
            },
          )
        }).collect();

      let res_headers = builder.create_vector(&headers);

      let msg = msg::FetchHttpResponse::create(
        builder,
        &msg::FetchHttpResponseArgs {
          id: req_id,
          headers: Some(res_headers),
          body: res.bytes.is_some(),
          ..Default::default()
        },
      );
      Ok(serialize_response(
        cmd_id,
        builder,
        msg::BaseArgs {
          msg: Some(msg.as_union_value()),
          msg_type: msg::Any::FetchHttpResponse,
          ..Default::default()
        },
      ))
    });

  unsafe {
    match EVENT_LOOP_HANDLE {
      Some(ref elh) => {
        elh.lock().unwrap().spawn(fut);
      }
      _ => panic!("event loop handle is NONE"),
    }
  };

  Box::new(fut2)
  // }

  // let builder = &mut FlatBufferBuilder::new();
  // let req_start_msg =
  //   msg::HttpRequestStart::create(builder, &msg::HttpRequestStartArgs { id: req_id });
  // ok_future(serialize_response(
  //   cmd_id,
  //   builder,
  //   msg::BaseArgs {
  //     msg: Some(req_start_msg.as_union_value()),
  //     msg_type: msg::Any::HttpRequestStart,
  //     ..Default::default()
  //   },
  // ))
}

fn handle_http_response(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  // println!("handle_http_response");
  let msg = base.msg_as_http_response().unwrap();
  let req_id = msg.id();

  let mut headers = HeaderMap::new();

  if let Some(msg_headers) = msg.headers() {
    for i in 0..msg_headers.len() {
      let h = msg_headers.get(i);
      headers.insert(
        HeaderName::from_bytes(h.key().unwrap().as_bytes()).unwrap(),
        h.value().unwrap().parse().unwrap(),
      );
    }
  }

  let mut chunk_recver: Option<mpsc::UnboundedReceiver<Vec<u8>>> = None;
  if msg.body() {
    let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
    {
      let mut bytes = rt.bytes.lock().unwrap();
      bytes.insert(req_id, sender);
    }
    chunk_recver = Some(recver);
  }

  let mut responses = rt.responses.lock().unwrap();
  match responses.remove(&req_id) {
    Some(mut sender) => {
      sender.send(JsHttpResponse {
        headers: headers,
        bytes: chunk_recver,
      });
    }
    _ => unimplemented!(),
  };

  ok_future(None)
}

fn handle_stream_chunk(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_stream_chunk().unwrap();
  let stream_id = msg.id();

  let mut bytes = rt.bytes.lock().unwrap();
  if (raw.data_len > 0) {
    match bytes.get_mut(&stream_id) {
      Some(mut sender) => {
        let bytes = unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec();
        match sender.unbounded_send(bytes.to_vec()) {
          Err(e) => println!("error sending chunk: {}", e),
          _ => {}
        }
      }
      _ => unimplemented!(),
    };
  }
  if (msg.done()) {
    bytes.remove(&stream_id);
  }

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

#[cfg(test)]
mod tests {
  use super::*;
  extern crate sourcemap;
  use self::sourcemap::{decode, DecodedMap, SourceMap};
  use std::fs;

  #[test]
  fn it_sm() {
    let mut f = fs::File::open("fly/packages/v8env/dist/v8env.js.map").unwrap();
    let sm = sourcemap::decode(&mut f).unwrap();
    match sm {
      DecodedMap::Regular(s) => {
        println!("got a regular map!");
        let t = s.lookup_token(12956, 20).unwrap();
        println!("token: {:?}", t);
        let (source, src_line, src_col, name) = t.to_tuple();
        println!("{:?} {}:{} {}", name, src_line, src_col, source);
      }
      DecodedMap::Index(s) => println!("got an index map!"),
    };
    assert!(true);
  }
}
