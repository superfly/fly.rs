extern crate libc;

use tokio;
use tokio::prelude::*;

use std::io;

use std::ffi::{CStr, CString};
use std::sync::{Arc, Mutex, Once, RwLock};

use self::fs::File;
use std::fs;
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

extern crate sha1; // SHA-1
extern crate sha2; // SHA-256, etc.
use self::sha1::Digest as Sha1Digest; // puts trait in scope
use self::sha1::Sha1;
use self::sha2::Digest; // puts trait in scope
use self::sha2::Sha256;

extern crate hyper;
extern crate r2d2;
extern crate r2d2_redis;
extern crate r2d2_sqlite;
extern crate rusqlite;
use self::r2d2_redis::redis;
use self::r2d2_sqlite::SqliteConnectionManager;

use self::hyper::body::Payload;
use self::hyper::client::HttpConnector;
use self::hyper::header::HeaderName;
use self::hyper::rt::{poll_fn, Future, Stream};
use self::hyper::HeaderMap;
use self::hyper::{Body, Client, Method, Request, StatusCode};

extern crate hyper_tls;
use self::hyper_tls::HttpsConnector;

use flatbuffers::FlatBufferBuilder;
use msg;

use errors::{FlyError, FlyResult};

use redis_stream;

extern crate log;

extern crate rand;
use self::rand::{thread_rng, Rng};

extern crate tokio_fs;

extern crate tokio_codec;
use self::tokio_codec::{BytesCodec, FramedRead};

extern crate trust_dns as dns;
extern crate trust_dns_proto as dns_proto;
use self::dns::client::ClientHandle; // necessary for trait to be in scope

#[derive(Debug)]
pub struct JsHttpResponse {
  pub headers: HeaderMap,
  pub status: StatusCode,
  pub bytes: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
}

#[derive(Debug)]
pub struct JsDnsResponse {
  pub op_code: dns::op::OpCode,
  pub message_type: dns::op::MessageType,
  pub response_code: dns::op::ResponseCode,
  pub answers: Vec<JsDnsRecord>,
  pub queries: Vec<JsDnsQuery>,
  pub authoritative: bool,
  pub truncated: bool,
}

#[derive(Debug)]
pub struct JsDnsRecord {
  pub name: dns::rr::Name,
  pub rdata: dns::rr::RData,
  pub dns_class: dns::rr::DNSClass,
  pub ttl: u32,
}

#[derive(Debug)]
pub struct JsDnsQuery {
  pub name: dns::rr::Name,
  pub rr_type: dns::rr::RecordType,
  pub dns_class: dns::rr::DNSClass,
}

#[derive(Debug, Copy, Clone)]
pub struct JsRuntime(pub *const js_runtime);
unsafe impl Send for JsRuntime {}
unsafe impl Sync for JsRuntime {}

impl JsRuntime {
  pub fn send(&self, buf: fly_buf, raw: Option<fly_buf>) {
    unsafe {
      js_send(
        self.0,
        buf,
        match raw {
          Some(r) => r,
          None => null_buf(),
        },
      )
    };
  }
  pub fn to_runtime(&self) -> &mut Runtime {
    let ptr = unsafe { js_get_data(self.0) };
    let rt_ptr = ptr as *mut Runtime;
    let rt_box = unsafe { Box::from_raw(rt_ptr) };
    Box::leak(rt_box)
  }
}

pub struct Runtime {
  pub ptr: JsRuntime,
  pub rt: Mutex<tokio::runtime::current_thread::Handle>,
  timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
  pub responses: Mutex<HashMap<u32, oneshot::Sender<JsHttpResponse>>>,
  pub dns_responses: Mutex<HashMap<u32, oneshot::Sender<JsDnsResponse>>>,
  pub bytes: Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>,
  pub http_client: Client<HttpsConnector<HttpConnector>, Body>,
  pub name: String,
}

static JSINIT: Once = Once::new();
static NEXT_RUNTIME_ID: AtomicUsize = ATOMIC_USIZE_INIT;

impl Runtime {
  pub fn new(name: Option<String>) -> Box<Self> {
    JSINIT.call_once(|| unsafe { js_init() });

    let (c, p) = oneshot::channel::<current_thread::Handle>();
    thread::Builder::new()
      .name(format!(
        "runtime-loop-{}",
        NEXT_RUNTIME_ID.fetch_add(1, Ordering::SeqCst)
      )).spawn(move || {
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
      }).unwrap();

    let mut rt_box = Box::new(Runtime {
      ptr: JsRuntime(0 as *const js_runtime),
      rt: Mutex::new(p.wait().unwrap()),
      timers: Mutex::new(HashMap::new()),
      responses: Mutex::new(HashMap::new()),
      dns_responses: Mutex::new(HashMap::new()),
      bytes: Mutex::new(HashMap::new()),
      http_client: Client::builder().build(HttpsConnector::new(4).unwrap()),
      name: name.unwrap_or("v8".to_string()),
    });

    (*rt_box).ptr.0 = unsafe {
      let ptr = js_runtime_new(
        *FLY_SNAPSHOT,
        rt_box.as_ref() as *const _ as *mut libc::c_void,
        msg_from_js,
        print_from_js,
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

extern crate tokio_io_pool;

#[cfg(debug_assertions)]
lazy_static! {
  static ref V8ENV_SNAPSHOT: Box<[u8]> = {
    let filename = "v8env/dist/v8env.js";
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let snap = unsafe {
      js_create_snapshot(
        CString::new(filename).unwrap().as_ptr(),
        CString::new(contents).unwrap().as_ptr(),
      )
    };
    let bytes: Vec<u8> =
      unsafe { slice::from_raw_parts(snap.ptr as *const u8, snap.len as usize) }.to_vec();
    bytes.into_boxed_slice()
  };
}

lazy_static_include_bytes!(V8ENV_SOURCEMAP, "v8env/dist/v8env.js.map");
#[cfg(not(debug_assertions))]
const V8ENV_SNAPSHOT: &'static [u8] = include_bytes!("../v8env.bin");

extern crate sourcemap;
use self::sourcemap::SourceMap;

pub static mut EVENT_LOOP_HANDLE: Option<tokio::runtime::TaskExecutor> = None;

lazy_static! {
  pub static ref RUNTIMES: RwLock<HashMap<String, Vec<Box<Runtime>>>> = RwLock::new(HashMap::new());
  static ref FLY_SNAPSHOT: fly_simple_buf = fly_simple_buf {
    ptr: V8ENV_SNAPSHOT.as_ptr() as *const i8,
    len: V8ENV_SNAPSHOT.len() as i32
  };
  static ref SQLITE_POOL: Arc<r2d2::Pool<SqliteConnectionManager>> = {
    let manager = SqliteConnectionManager::file("play.db");
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    Arc::new(pool)
  };
  static ref DNS_RESOLVER: Mutex<dns::client::BasicClientHandle<dns_proto::xfer::DnsMultiplexerSerialResponse>> = {
    let (stream, handle) = dns::udp::UdpClientStream::new(([8, 8, 8, 8], 53).into());
    let (bg, mut client) = dns::client::ClientFuture::new(stream, handle, None);
    unsafe {
      EVENT_LOOP_HANDLE.as_ref().unwrap().spawn(bg)
      //   bg.map_err(|e| println!("error getting dns client: {}", e))
      //     .and_then(move |client| {
      //       println!("got a client from the spawned future :D");
      //       tx.send(client);
      //       Ok(())
      //     }),
      // )
    };
    Mutex::new(client)
    // Mutex::new(rx.wait().unwrap())
  };
  static ref SM_CHAN: Mutex<
    stdmspc::Sender<(
      Vec<(u32, u32, String, String)>,
      oneshot::Sender<Vec<(u32, u32, String, String)>>
    )>,
  > = {
    let (sender, receiver) = stdmspc::channel::<(
      Vec<(u32, u32, String, String)>,
      oneshot::Sender<Vec<(u32, u32, String, String)>>,
    )>();
    thread::Builder::new()
      .name("sourcemapper".to_string())
      .spawn(move || {
        let sm = SourceMap::from_reader(*V8ENV_SOURCEMAP).unwrap();
        for tup in receiver.iter() {
          let ch = tup.1;
          let v = tup.0;
          ch.send(
            v.iter()
              .map(|(line, col, name, filename)| {
                if filename == "v8env.js" {
                  return match sm.lookup_token(*line, *col) {
                    Some(t) => {
                      let newline = t.get_src_line();
                      let newcol = t.get_src_col();
                      let newfilename = match t.get_source() {
                        Some(s) => String::from(s),
                        None => filename.clone(),
                      };
                      (newline, newcol, name.clone(), newfilename)
                    }
                    None => (*line, *col, name.clone(), filename.clone()),
                  };
                }
                (*line, *col, name.clone(), filename.clone())
              }).collect(),
          ).unwrap();
        }
      }).unwrap();
    Mutex::new(sender)
  };
}

// Buf represents a byte array returned from a "Op".
// The message might be empty (which will be translated into a null object on
// the javascript side) or it is a heap allocated opaque sequence of bytes.
// Usually a flatbuffer message.
pub type Buf = Option<Box<[u8]>>;

// JS promises in Deno map onto a specific Future
// which yields either a DenoError or a byte array.
type Op = Future<Item = Buf, Error = FlyError> + Send;

type Handler = fn(rt: &Runtime, base: &msg::Base, raw_buf: fly_buf) -> Box<Op>;

use std::slice;

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
    msg::Any::CryptoRandomValues => handle_crypto_random_values,
    msg::Any::SourceMap => handle_source_map,
    msg::Any::DataPut => handle_data_put,
    msg::Any::DataGet => handle_data_get,
    msg::Any::DataDel => handle_data_del,
    msg::Any::DataDropCollection => handle_data_drop_coll,
    msg::Any::DnsQuery => handle_dns_query,
    msg::Any::DnsResponse => handle_dns_response,
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

pub extern "C" fn print_from_js(raw: *const js_runtime, lvl: i8, msg: *const libc::c_char) {
  let rt = from_c(raw);
  let msg = unsafe { CStr::from_ptr(msg).to_string_lossy().into_owned() };

  let lvl = match lvl {
    0 => log::Level::Error,
    1 => log::Level::Warn,
    2 => log::Level::Info,
    3 => log::Level::Debug,
    4 => log::Level::Trace,
    _ => log::Level::Info,
  };

  log!(lvl, "console/{}: {}", &rt.name, &msg);
}

fn handle_timer_start(rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
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
  let vec = data.to_vec();
  Some(vec.into_boxed_slice())
}

fn remove_timer(ptr: JsRuntime, timer_id: u32) {
  let rt = from_c(ptr.0);
  rt.timers.lock().unwrap().remove(&timer_id);
}

fn handle_timer_clear(rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_timer_clear().unwrap();
  println!("handle_timer_clear");
  remove_timer(rt.ptr, msg.id());
  ok_future(None)
}

fn handle_source_map(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
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

fn handle_crypto_random_values(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_crypto_random_values().unwrap();

  let len = msg.len() as usize;
  let mut v = vec![0u8; len];
  let arr = v.as_mut_slice();

  thread_rng().fill(arr);

  let builder = &mut FlatBufferBuilder::new();
  let ret_buffer = builder.create_vector(arr);

  let crypto_rand = msg::CryptoRandomValuesReady::create(
    builder,
    &msg::CryptoRandomValuesReadyArgs {
      buffer: Some(ret_buffer),
      ..Default::default()
    },
  );

  ok_future(serialize_response(
    cmd_id,
    builder,
    msg::BaseArgs {
      msg: Some(crypto_rand.as_union_value()),
      msg_type: msg::Any::CryptoRandomValuesReady,
      ..Default::default()
    },
  ))
}

fn handle_crypto_digest(_rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
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

    let crypto_ready = msg::CryptoDigestReady::create(
      builder,
      &msg::CryptoDigestReadyArgs {
        buffer: Some(bytes_vec),
        ..Default::default()
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

use super::NEXT_EVENT_ID;
use std::str;

use std::ops::Deref;
fn handle_cache_set(rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  println!("CACHE SET");
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_cache_set().unwrap();
  let key = msg.key().unwrap().to_string();

  let stream_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
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
            Ok(_r) => {}
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

fn handle_cache_get(rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_cache_get().unwrap();

  let stream_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

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

fn handle_file_request(rt: &Runtime, cmd_id: u32, url: &str) -> Box<Op> {
  let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;
  let rtptr = rt.ptr;

  let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

  let rtptr2 = rtptr.clone();

  let path: String = url.chars().skip(7).collect();

  let meta = match fs::metadata(path.clone()) {
    Ok(m) => m,
    Err(e) => return odd_future(e.into()),
  };

  println!("META: {:?}", meta);

  if meta.is_file() {
    let fut = future::lazy(move || {
      tokio_fs::File::open(path).then(
        move |fileerr: Result<tokio_fs::File, io::Error>| -> Result<(), ()> {
          if let Err(err) = fileerr {
            p.send(Err(err.into()));
            return Ok(());
          }

          let file = fileerr.unwrap(); // should be safe.

          let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
          let bytes_rx = Some(rx);
          let mut bytes = from_c(rtptr2.0).bytes.lock().unwrap();
          bytes.insert(req_id, tx);

          p.send(Ok(JsHttpResponse {
            headers: HeaderMap::new(),
            status: StatusCode::OK,
            bytes: bytes_rx,
          }));

          let rt = from_c(rtptr.0); // like a clone
          rt.rt.lock().unwrap().spawn(future::lazy(move || {
            let innerfut = Box::new(
              FramedRead::new(file, BytesCodec::new())
                .map_err(|e| println!("error reading file chunk! {}", e))
                .for_each(move |mut chunk| {
                  let builder = &mut FlatBufferBuilder::new();
                  let chunk_msg = msg::StreamChunk::create(
                    builder,
                    &msg::StreamChunkArgs {
                      id: req_id,
                      done: false,
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
                        data_ptr: chunk.as_mut_ptr(),
                        data_len: chunk.len(),
                      },
                    )
                  };
                  Ok(())
                }).and_then(move |_| {
                  let builder = &mut FlatBufferBuilder::new();
                  let chunk_msg = msg::StreamChunk::create(
                    builder,
                    &msg::StreamChunkArgs {
                      id: req_id,
                      done: true,
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

            unsafe {
              match EVENT_LOOP_HANDLE {
                Some(ref mut elh) => {
                  elh.spawn(innerfut);
                }
                None => panic!("requires a multi-threaded event loop"),
              }
            };
            Ok(())
          }));

          Ok(())
        },
      );
      Ok(())
    });
    unsafe { EVENT_LOOP_HANDLE.as_mut().unwrap().spawn(fut) };
  } else {
    let fut = tokio_fs::read_dir(path).then(move |read_dir_err| {
      if let Err(e) = read_dir_err {
        p.send(Err(e.into()));
        return Ok(());
      }
      let read_dir = read_dir_err.unwrap();
      let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
      let mut bytes = from_c(rtptr2.0).bytes.lock().unwrap();
      bytes.insert(req_id, tx);

      p.send(Ok(JsHttpResponse {
        headers: HeaderMap::new(),
        status: StatusCode::OK,
        bytes: Some(rx),
      }));

      let fut = read_dir
        .map_err(|e| println!("error read_dir stream: {}", e))
        .for_each(move |entry| {
          let rt = from_c(rtptr.0); // like a clone
          rt.rt.lock().unwrap().spawn(future::lazy(move || {
            let entrypath = entry.path();
            let pathstr = format!("{}\n", entrypath.to_str().unwrap());
            let pathbytes = pathstr.as_bytes();
            let builder = &mut FlatBufferBuilder::new();
            let chunk_msg = msg::StreamChunk::create(
              builder,
              &msg::StreamChunkArgs {
                id: req_id,
                done: false,
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
                  data_ptr: pathbytes.as_ptr() as *mut u8,
                  data_len: pathbytes.len(),
                },
              )
            };
            Ok(())
          }));
          Ok(())
        }).and_then(move |_| {
          let rt = from_c(rtptr.0); // like a clone
          rt.rt.lock().unwrap().spawn(future::lazy(move || {
            let builder = &mut FlatBufferBuilder::new();
            let chunk_msg = msg::StreamChunk::create(
              builder,
              &msg::StreamChunkArgs {
                id: req_id,
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
          }));
          Ok(())
        });
      unsafe { EVENT_LOOP_HANDLE.as_mut().unwrap().spawn(fut) };
      Ok(())
    });
    unsafe { EVENT_LOOP_HANDLE.as_mut().unwrap().spawn(fut) };
  };

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
          status: res.status.as_u16(),
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

  // unsafe {
  //   match EVENT_LOOP_HANDLE {
  //     Some(ref mut elh) => {
  //       elh.spawn(fut);
  //     }
  //     None => {
  //       rt.rt.lock().unwrap().spawn(fut);
  //     }
  //   }
  // };

  Box::new(fut2)
}

fn handle_dns_query(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  println!("handle dns");
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_dns_query().unwrap();

  let query_type = match msg.rr_type() {
    msg::DnsRecordType::A => dns::rr::RecordType::A,
    msg::DnsRecordType::AAAA => dns::rr::RecordType::AAAA,
    msg::DnsRecordType::ANY => dns::rr::RecordType::ANY,
    msg::DnsRecordType::AXFR => dns::rr::RecordType::AXFR,
    msg::DnsRecordType::CAA => dns::rr::RecordType::CAA,
    msg::DnsRecordType::CNAME => dns::rr::RecordType::CNAME,
    msg::DnsRecordType::IXFR => dns::rr::RecordType::IXFR,
    msg::DnsRecordType::MX => dns::rr::RecordType::MX,
    msg::DnsRecordType::NS => dns::rr::RecordType::NS,
    msg::DnsRecordType::NULL => dns::rr::RecordType::NULL,
    msg::DnsRecordType::OPT => dns::rr::RecordType::OPT,
    msg::DnsRecordType::PTR => dns::rr::RecordType::PTR,
    msg::DnsRecordType::SOA => dns::rr::RecordType::SOA,
    msg::DnsRecordType::SRV => dns::rr::RecordType::SRV,
    msg::DnsRecordType::TLSA => dns::rr::RecordType::TLSA,
    msg::DnsRecordType::TXT => dns::rr::RecordType::TXT,
  };

  Box::new(
    DNS_RESOLVER
      .lock()
      .unwrap()
      .query(
        msg.name().unwrap().parse().unwrap(),
        dns::rr::DNSClass::IN,
        query_type,
      ).map_err(|e| format!("dns query error: {}", e).into())
      .and_then(move |res| {
        // println!("got a dns response! {:?}", res);
        for q in res.queries() {
          println!("queried: {:?}", q);
        }
        let builder = &mut FlatBufferBuilder::new();
        let answers: Vec<_> = res
          .answers()
          .iter()
          .map(|ans| {
            println!("answer: {:?}", ans);
            use self::dns::rr::{DNSClass, RData, RecordType};
            let name = builder.create_string(&ans.name().to_utf8());
            let rr_type = match ans.rr_type() {
              RecordType::A => msg::DnsRecordType::A,
              RecordType::AAAA => msg::DnsRecordType::AAAA,
              RecordType::AXFR => msg::DnsRecordType::AXFR,
              RecordType::CAA => msg::DnsRecordType::CAA,
              RecordType::CNAME => msg::DnsRecordType::CNAME,
              RecordType::IXFR => msg::DnsRecordType::IXFR,
              RecordType::MX => msg::DnsRecordType::MX,
              RecordType::NS => msg::DnsRecordType::NS,
              RecordType::NULL => msg::DnsRecordType::NULL,
              RecordType::OPT => msg::DnsRecordType::OPT,
              RecordType::PTR => msg::DnsRecordType::PTR,
              RecordType::SOA => msg::DnsRecordType::SOA,
              RecordType::SRV => msg::DnsRecordType::SRV,
              RecordType::TLSA => msg::DnsRecordType::TLSA,
              RecordType::TXT => msg::DnsRecordType::TXT,
              _ => unimplemented!(),
            };
            let dns_class = match ans.dns_class() {
              DNSClass::IN => msg::DnsClass::IN,
              DNSClass::CH => msg::DnsClass::CH,
              DNSClass::HS => msg::DnsClass::HS,
              DNSClass::NONE => msg::DnsClass::NONE,
              DNSClass::ANY => msg::DnsClass::ANY,
              _ => unimplemented!(),
            };
            let rdata_type = match ans.rdata() {
              RData::A(_) => msg::DnsRecordData::DnsA,
              RData::AAAA(_) => msg::DnsRecordData::DnsAaaa,
              RData::CNAME(_) => msg::DnsRecordData::DnsCname,
              RData::MX(_) => msg::DnsRecordData::DnsMx,
              RData::NS(_) => msg::DnsRecordData::DnsNs,
              RData::PTR(_) => msg::DnsRecordData::DnsPtr,
              RData::SOA(_) => msg::DnsRecordData::DnsSoa,
              RData::SRV(_) => msg::DnsRecordData::DnsSrv,
              RData::TXT(_) => msg::DnsRecordData::DnsTxt,
              _ => unimplemented!(),
            };
            let rdata = match ans.rdata() {
              RData::A(ip) => {
                let ipstr = builder.create_string(&ip.to_string());
                msg::DnsA::create(
                  builder,
                  &msg::DnsAArgs {
                    ip: Some(ipstr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::AAAA(ip) => {
                let ipstr = builder.create_string(&ip.to_string());
                msg::DnsAaaa::create(
                  builder,
                  &msg::DnsAaaaArgs {
                    ip: Some(ipstr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::CNAME(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsCname::create(
                  builder,
                  &msg::DnsCnameArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::MX(mx) => {
                let exstr = builder.create_string(&mx.exchange().to_utf8());
                msg::DnsMx::create(
                  builder,
                  &msg::DnsMxArgs {
                    exchange: Some(exstr),
                    preference: mx.preference(),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::NS(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsNs::create(
                  builder,
                  &msg::DnsNsArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::PTR(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsPtr::create(
                  builder,
                  &msg::DnsPtrArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::SOA(soa) => {
                let mnamestr = builder.create_string(&soa.mname().to_utf8());
                let rnamestr = builder.create_string(&soa.rname().to_utf8());
                msg::DnsSoa::create(
                  builder,
                  &msg::DnsSoaArgs {
                    mname: Some(mnamestr),
                    rname: Some(rnamestr),
                    serial: soa.serial(),
                    refresh: soa.refresh(),
                    retry: soa.retry(),
                    expire: soa.expire(),
                    minimum: soa.minimum(),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::SRV(srv) => {
                let targetstr = builder.create_string(&srv.target().to_utf8());
                msg::DnsSrv::create(
                  builder,
                  &msg::DnsSrvArgs {
                    priority: srv.priority(),
                    weight: srv.weight(),
                    port: srv.port(),
                    target: Some(targetstr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::TXT(txt) => {
                let coll: Vec<_> = txt
                  .iter()
                  .map(|t| {
                    let d = builder.create_vector(&Vec::from(t.clone()));
                    msg::DnsTxtData::create(
                      builder,
                      &msg::DnsTxtDataArgs {
                        data: Some(d),
                        ..Default::default()
                      },
                    )
                  }).collect();
                let data = builder.create_vector(&coll);

                msg::DnsTxt::create(
                  builder,
                  &msg::DnsTxtArgs {
                    data: Some(data),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              _ => unimplemented!(),
            };

            msg::DnsRecord::create(
              builder,
              &msg::DnsRecordArgs {
                name: Some(name),
                rr_type: rr_type,
                dns_class: dns_class,
                ttl: ans.ttl(),
                rdata_type: rdata_type,
                rdata: Some(rdata),
                ..Default::default()
              },
            )
          }).collect();

        let res_answers = builder.create_vector(&answers);
        let dns_msg = msg::DnsResponse::create(
          builder,
          &msg::DnsResponseArgs {
            op_code: msg::DnsOpCode::Query,
            message_type: msg::DnsMessageType::Response,
            authoritative: res.authoritative(),
            truncated: res.truncated(),
            // response_code: ,
            answers: Some(res_answers),
            // done: body.is_end_stream(),
            ..Default::default()
          },
        );

        Ok(serialize_response(
          cmd_id,
          builder,
          msg::BaseArgs {
            msg: Some(dns_msg.as_union_value()),
            msg_type: msg::Any::DnsResponse,
            ..Default::default()
          },
        ))
      }),
  )
}

fn handle_http_request(rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_http_request().unwrap();

  let url = msg.url().unwrap();
  if url.starts_with("file://") {
    return handle_file_request(rt, cmd_id, url);
  }

  let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;
  let rtptr = rt.ptr;

  let req_body: Body;
  if msg.body() {
    unimplemented!();
  } else {
    req_body = Body::empty();
  }

  let mut req = Request::new(req_body);
  {
    let uri: hyper::Uri = url.parse().unwrap();
    // println!("url: {:?}", uri);
    *req.uri_mut() = uri;
    *req.method_mut() = match msg.method() {
      msg::HttpMethod::Get => Method::GET,
      msg::HttpMethod::Post => Method::POST,
      _ => unimplemented!(),
    };

    let msg_headers = msg.headers().unwrap();
    let headers = req.headers_mut();
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
        status: parts.status,
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
          }).map_err(|e: hyper::Error| println!("hyper error: {}",e)),
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
          status: res.status.as_u16(),
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
      Some(ref mut elh) => {
        elh.spawn(fut);
      }
      None => {
        rt.rt.lock().unwrap().spawn(fut);
      }
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

fn handle_http_response(rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_http_response().unwrap();
  let req_id = msg.id();

  let status = match StatusCode::from_u16(msg.status()) {
    Ok(s) => s,
    Err(e) => return odd_future(format!("{}", e).into()),
  };

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
    Some(sender) => {
      sender.send(JsHttpResponse {
        headers: headers,
        status: status,
        bytes: chunk_recver,
      });
    }
    _ => unimplemented!(),
  };

  ok_future(None)
}

fn handle_dns_response(rt: &Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_dns_response().unwrap();
  let req_id = msg.id();

  let op_code = match msg.op_code() {
    msg::DnsOpCode::Query => dns::op::OpCode::Query,
    msg::DnsOpCode::Status => dns::op::OpCode::Status,
    msg::DnsOpCode::Notify => dns::op::OpCode::Notify,
    msg::DnsOpCode::Update => dns::op::OpCode::Update,
  };

  let res_code = msg.response_code() as u16;

  let message_type = match msg.message_type() {
    msg::DnsMessageType::Query => dns::op::MessageType::Query,
    msg::DnsMessageType::Response => dns::op::MessageType::Response,
  };

  use self::dns::rr::RData;

  let queries: Vec<JsDnsQuery> = if let Some(msg_queries) = msg.queries() {
    let qlen = msg_queries.len();
    let mut queries: Vec<JsDnsQuery> = Vec::with_capacity(qlen);
    for i in 0..qlen {
      let q = msg_queries.get(i);

      let rr_type = match q.rr_type() {
        msg::DnsRecordType::A => dns::rr::RecordType::A,
        msg::DnsRecordType::AAAA => dns::rr::RecordType::AAAA,
        msg::DnsRecordType::ANY => dns::rr::RecordType::ANY,
        msg::DnsRecordType::AXFR => dns::rr::RecordType::AXFR,
        msg::DnsRecordType::CAA => dns::rr::RecordType::CAA,
        msg::DnsRecordType::CNAME => dns::rr::RecordType::CNAME,
        msg::DnsRecordType::IXFR => dns::rr::RecordType::IXFR,
        msg::DnsRecordType::MX => dns::rr::RecordType::MX,
        msg::DnsRecordType::NS => dns::rr::RecordType::NS,
        msg::DnsRecordType::NULL => dns::rr::RecordType::NULL,
        msg::DnsRecordType::OPT => dns::rr::RecordType::OPT,
        msg::DnsRecordType::PTR => dns::rr::RecordType::PTR,
        msg::DnsRecordType::SOA => dns::rr::RecordType::SOA,
        msg::DnsRecordType::SRV => dns::rr::RecordType::SRV,
        msg::DnsRecordType::TLSA => dns::rr::RecordType::TLSA,
        msg::DnsRecordType::TXT => dns::rr::RecordType::TXT,
      };

      let dns_class = match q.dns_class() {
        msg::DnsClass::IN => dns::rr::DNSClass::IN,
        msg::DnsClass::CH => dns::rr::DNSClass::CH,
        msg::DnsClass::HS => dns::rr::DNSClass::HS,
        msg::DnsClass::NONE => dns::rr::DNSClass::NONE,
        msg::DnsClass::ANY => dns::rr::DNSClass::ANY,
        _ => unimplemented!(),
      };

      queries.push(JsDnsQuery {
        name: q.name().unwrap().parse().unwrap(),
        rr_type: rr_type,
        dns_class: dns_class,
      });
    }
    vec![]
  } else {
    vec![]
  };

  let answers = if let Some(msg_answers) = msg.answers() {
    let anslen = msg_answers.len();
    let mut answers: Vec<JsDnsRecord> = Vec::with_capacity(anslen);
    for i in 0..anslen {
      let ans = msg_answers.get(i);

      let dns_class = match ans.dns_class() {
        msg::DnsClass::IN => dns::rr::DNSClass::IN,
        msg::DnsClass::CH => dns::rr::DNSClass::CH,
        msg::DnsClass::HS => dns::rr::DNSClass::HS,
        msg::DnsClass::NONE => dns::rr::DNSClass::NONE,
        msg::DnsClass::ANY => dns::rr::DNSClass::ANY,
        _ => unimplemented!(),
      };

      let rdata: RData = match ans.rdata_type() {
        msg::DnsRecordData::DnsA => {
          let d = ans.rdata_as_dns_a().unwrap();
          RData::A(d.ip().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsAaaa => {
          let d = ans.rdata_as_dns_aaaa().unwrap();
          RData::AAAA(d.ip().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsCname => {
          let d = ans.rdata_as_dns_cname().unwrap();
          RData::CNAME(d.name().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsMx => {
          let d = ans.rdata_as_dns_mx().unwrap();
          RData::MX(dns::rr::rdata::mx::MX::new(
            d.preference(),
            d.exchange().unwrap().parse().unwrap(),
          ))
        }
        msg::DnsRecordData::DnsNs => {
          let d = ans.rdata_as_dns_ns().unwrap();
          RData::NS(d.name().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsPtr => {
          let d = ans.rdata_as_dns_ptr().unwrap();
          RData::PTR(d.name().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsSoa => {
          let d = ans.rdata_as_dns_soa().unwrap();
          RData::SOA(dns::rr::rdata::soa::SOA::new(
            d.mname().unwrap().parse().unwrap(),
            d.rname().unwrap().parse().unwrap(),
            d.serial(),
            d.refresh(),
            d.retry(),
            d.expire(),
            d.minimum(),
          ))
        }
        msg::DnsRecordData::DnsSrv => {
          let d = ans.rdata_as_dns_srv().unwrap();
          RData::SRV(dns::rr::rdata::srv::SRV::new(
            d.priority(),
            d.weight(),
            d.port(),
            d.target().unwrap().parse().unwrap(),
          ))
        }
        msg::DnsRecordData::DnsTxt => {
          let d = ans.rdata_as_dns_txt().unwrap();
          let tdata = d.data().unwrap();
          let data_len = tdata.len();
          let mut txtdata: Vec<String> = Vec::with_capacity(data_len);
          for i in 0..data_len {
            let td = tdata.get(i);
            txtdata.push(String::from_utf8_lossy(td.data().unwrap()).to_string());
          }
          RData::TXT(dns::rr::rdata::txt::TXT::new(txtdata))
        }
        _ => unimplemented!(),
      };

      answers.push(JsDnsRecord {
        name: ans.name().unwrap().parse().unwrap(),
        dns_class: dns_class,
        ttl: ans.ttl(),
        rdata: rdata,
      });
    }
    answers
  } else {
    vec![]
  };

  let mut responses = rt.dns_responses.lock().unwrap();
  match responses.remove(&req_id) {
    Some(sender) => {
      sender.send(JsDnsResponse {
        op_code: op_code,
        authoritative: msg.authoritative(),
        truncated: msg.truncated(),
        response_code: res_code.into(),
        message_type: message_type,
        queries: queries,
        answers: answers,
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
  if raw.data_len > 0 {
    match bytes.get_mut(&stream_id) {
      Some(sender) => {
        let bytes = unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec();
        match sender.unbounded_send(bytes.to_vec()) {
          Err(e) => println!("error sending chunk: {}", e),
          _ => {}
        }
      }
      _ => unimplemented!(),
    };
  }
  if msg.done() {
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

fn handle_data_put(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_data_put().unwrap();
  let coll = msg.collection().unwrap().to_string();
  let key = msg.key().unwrap().to_string();
  let value = msg.json().unwrap().to_string();

  Box::new(future::lazy(move || -> FlyResult<Buf> {
    let pool = Arc::clone(&SQLITE_POOL);
    let con = pool.get().unwrap(); // TODO: no unwrap

    create_collection(&*con, &coll).unwrap();

    match con.execute(
      format!("INSERT OR REPLACE INTO {} VALUES (?, ?)", coll).as_str(),
      &[&key, &value],
    ) {
      Ok(r) => {
        println!("PUT returned: {}", r);
        Ok(None)
      }
      Err(e) => Err(format!("{}", e).into()),
    }
  }))
}

fn handle_data_get(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_data_get().unwrap();
  let coll = msg.collection().unwrap().to_string();
  let key = msg.key().unwrap().to_string();

  Box::new(future::lazy(move || -> FlyResult<Buf> {
    let pool = Arc::clone(&SQLITE_POOL);
    let con = pool.get().unwrap(); // TODO: no unwrap

    create_collection(&*con, &coll).unwrap();

    match con.query_row::<String, _>(
      format!("SELECT obj FROM {} WHERE key == ?", coll).as_str(),
      &[&key],
      |row| row.get(0),
    ) {
      Err(e) => match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(format!("{}", e).into()),
      },
      Ok(s) => {
        let builder = &mut FlatBufferBuilder::new();
        let json = builder.create_string(&s);
        let msg = msg::DataGetReady::create(
          builder,
          &msg::DataGetReadyArgs {
            json: Some(json),
            ..Default::default()
          },
        );
        Ok(serialize_response(
          cmd_id,
          builder,
          msg::BaseArgs {
            msg: Some(msg.as_union_value()),
            msg_type: msg::Any::DataGetReady,
            ..Default::default()
          },
        ))
      }
    }
  }))
}

fn handle_data_del(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_data_del().unwrap();
  let coll = msg.collection().unwrap().to_string();
  let key = msg.key().unwrap().to_string();

  Box::new(future::lazy(move || -> FlyResult<Buf> {
    let pool = Arc::clone(&SQLITE_POOL);
    let con = pool.get().unwrap(); // TODO: no unwrap

    create_collection(&*con, &coll).unwrap();

    match con.execute(
      format!("DELETE FROM {} WHERE key == ?", coll).as_str(),
      &[&key],
    ) {
      Ok(_) => Ok(None),
      Err(e) => Err(format!("{}", e).into()),
    }
  }))
}

fn handle_data_drop_coll(_rt: &Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_data_del().unwrap();
  let coll = msg.collection().unwrap().to_string();

  Box::new(future::lazy(move || -> FlyResult<Buf> {
    let pool = Arc::clone(&SQLITE_POOL);
    let con = pool.get().unwrap(); // TODO: no unwrap

    match con.execute(format!("DROP TABLE IF EXISTS {}", coll).as_str(), &[]) {
      Ok(_) => Ok(None),
      Err(e) => Err(format!("{}", e).into()),
    }
  }))
}

fn create_collection(con: &rusqlite::Connection, name: &String) -> rusqlite::Result<usize> {
  con.execute(
    format!(
      "CREATE TABLE IF NOT EXISTS {} (key TEXT PRIMARY KEY NOT NULL, obj JSON NOT NULL)",
      name
    ).as_str(),
    &[],
  )
}
