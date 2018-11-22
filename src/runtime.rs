extern crate http;
extern crate libc;

use tokio;

use tokio::runtime::current_thread;

use std::io;

use std::ffi::{CStr, CString};
use std::sync::{Mutex, Once};

use self::fs::File;
use std::fs;
use std::io::Read;

use libfly::*;

use std::sync::mpsc as stdmspc;

use futures::sync::{mpsc, oneshot};
use std::collections::HashMap;

use std::thread;

use tokio::timer::Delay;

use std::time::{Duration, Instant};

use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

use futures::future;

use std::ptr;

extern crate sha1; // SHA-1
extern crate sha2; // SHA-256, etc.
#[allow(unused_imports)]
use self::sha1::Digest as Sha1Digest; // puts trait in scope
use self::sha1::Sha1;

#[allow(unused_imports)]
use self::sha2::Digest; // puts trait in scope
use self::sha2::Sha256;

extern crate hyper;

use self::hyper::body::Payload;
use self::hyper::client::HttpConnector;
use self::hyper::header::HeaderName;
use self::hyper::rt::{Future, Stream};
use self::hyper::HeaderMap;
use self::hyper::{Body, Client, Method, Request, StatusCode};

extern crate hyper_tls;
use self::hyper_tls::HttpsConnector;

use flatbuffers::FlatBufferBuilder;
use msg;

use errors::{FlyError, FlyResult};

extern crate log;

extern crate rand;
use self::rand::{thread_rng, Rng};

use tokio::codec::{BytesCodec, FramedRead};
extern crate bytes;
use self::bytes::BytesMut;

use cache_store;
use data_store;
use ops; // src/ops/
use utils::*;

use postgres_data;
use redis_cache;
use sqlite_cache;
use sqlite_data;

// use settings::{};
use settings::{CacheStore, DataStore, Settings};

use super::{NEXT_EVENT_ID, NEXT_FUTURE_ID};
use std::str;

extern crate trust_dns as dns;

#[derive(Debug)]
pub enum JsBody {
  Stream(mpsc::UnboundedReceiver<Vec<u8>>),
  BytesStream(mpsc::UnboundedReceiver<BytesMut>),
  HyperBody(Body),
  Static(Vec<u8>),
}

#[derive(Debug)]
pub struct JsHttpResponse {
  pub headers: HeaderMap,
  pub status: StatusCode,
  pub body: Option<JsBody>,
}

#[derive(Debug)]
pub struct JsHttpRequest {
  pub id: u32,
  pub method: http::Method,
  pub url: String,
  pub headers: HeaderMap,
  pub body: Option<JsBody>,
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

  pub fn send_error(&self, cmd_id: u32, err: FlyError) {
    let buf = build_error(cmd_id, err).unwrap();
    self.send(fly_buf_from(buf), None);
  }

  pub fn to_runtime<'a>(&self) -> &'a mut Runtime {
    unsafe { Runtime::from_raw(self.0) }
  }
}

pub struct Runtime {
  pub ptr: JsRuntime,
  pub name: String,
  pub event_loop: Mutex<current_thread::Handle>,
  timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
  pub responses: Mutex<HashMap<u32, oneshot::Sender<JsHttpResponse>>>,
  pub dns_responses: Mutex<HashMap<u32, oneshot::Sender<ops::dns::JsDnsResponse>>>,
  pub streams: Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>,
  // pub http_client: Client<HttpsConnector<HttpConnector>, Body>,
  pub cache_store: Box<cache_store::CacheStore + 'static + Send>,
  pub data_store: Box<data_store::DataStore + 'static + Send>,
  pub fetch_events: Option<mpsc::UnboundedSender<JsHttpRequest>>,
  pub resolv_events: Option<mpsc::UnboundedSender<ops::dns::JsDnsRequest>>,
  ready_ch: Option<oneshot::Sender<()>>,
  quit_ch: Option<oneshot::Receiver<()>>,
}

static JSINIT: Once = Once::new();
static NEXT_RUNTIME_ID: AtomicUsize = ATOMIC_USIZE_INIT;

fn init_event_loop() -> (
  current_thread::Handle,
  oneshot::Sender<()>,
  oneshot::Receiver<()>,
) {
  let (c, p) = oneshot::channel::<(
    current_thread::Handle,
    oneshot::Sender<()>,
    oneshot::Receiver<()>,
  )>();
  thread::Builder::new()
    .name(format!(
      "runtime-loop-{}",
      NEXT_RUNTIME_ID.fetch_add(1, Ordering::SeqCst)
    )).spawn(move || {
      // let mut el = tokio::runtime::Builder::new()
      //   .keep_alive(Some(Duration::from_secs(60)))
      //   .core_threads(1)
      //   .build()
      //   .unwrap();
      let mut el = current_thread::Runtime::new().unwrap();
      let (txready, rxready) = oneshot::channel::<()>();
      let (txquit, rxquit) = oneshot::channel::<()>();

      c.send((el.handle(), txready, rxquit)).unwrap();

      debug!("sent loop handle");

      // match el.block_on(rxready.map_err(|_| error!("error recving ready signal for runtime"))) {
      //   Ok(_) => warn!("stopped blocking on ready chan"),
      //   Err(_) => error!("error blocking on ready chan!"),
      // };

      // keep it alive at least until all scripts are evaled
      el.spawn(
        rxready
          .map_err(|_| error!("error recving ready signal for runtime"))
          .and_then(|_| Ok(warn!("ready ch received!"))),
      );

      // el.block_on_all(
      //   rxready
      //     .map_err(|_| error!("error recving ready signal for runtime"))
      //     .and_then(|_| Ok(warn!("ready ch received!"))),
      // ).unwrap();

      match el.run() {
        Ok(_) => warn!("runtime event loop ran fine"),
        Err(e) => error!("error running runtime event loop: {}", e),
      };
      warn!("Event loop has run its course.");
      match txquit.send(()) {
        Ok(_) => warn!("Sent quit () in channel successfully."),
        Err(_) => error!("error sending quit signal for runtime"),
      };
    }).unwrap();
  p.wait().unwrap()
}

impl Runtime {
  pub fn new(name: Option<String>, settings: &Settings) -> Box<Runtime> {
    JSINIT.call_once(|| unsafe { js_init() });

    let (rthandle, txready, rxquit) = init_event_loop();

    let mut rt = Box::new(Runtime {
      ptr: JsRuntime(ptr::null() as *const js_runtime),
      name: name.unwrap_or("v8".to_string()),
      event_loop: Mutex::new(rthandle.clone()),
      ready_ch: Some(txready),
      quit_ch: Some(rxquit),
      timers: Mutex::new(HashMap::new()),
      responses: Mutex::new(HashMap::new()),
      dns_responses: Mutex::new(HashMap::new()),
      streams: Mutex::new(HashMap::new()),
      fetch_events: None,
      resolv_events: None,
      cache_store: match settings.cache_store {
        Some(ref store) => match store {
          CacheStore::Sqlite(conf) => {
            Box::new(sqlite_cache::SqliteCacheStore::new(conf.filename.clone()))
          }
          CacheStore::Redis(conf) => Box::new(redis_cache::RedisCacheStore::new(&conf)),
        },
        None => Box::new(sqlite_cache::SqliteCacheStore::new("cache.db".to_string())),
      },
      data_store: match settings.data_store {
        Some(ref store) => match store {
          DataStore::Sqlite(conf) => {
            Box::new(sqlite_data::SqliteDataStore::new(conf.filename.clone()))
          }
          DataStore::Postgres(conf) => Box::new(postgres_data::PostgresDataStore::new(&conf)),
        },
        None => Box::new(sqlite_data::SqliteDataStore::new("data.db".to_string())),
      },
    });

    (*rt).ptr.0 = unsafe {
      let ptr = js_runtime_new(js_runtime_options {
        snapshot: *FLY_SNAPSHOT,
        data: rt.as_ref() as *const _ as *mut libc::c_void,
        recv_cb: msg_from_js,
        print_cb: print_from_js,
        soft_memory_limit: 128,
        hard_memory_limit: 256,
      });
      let cfilename = CString::new("fly_main.js").unwrap();
      let cscript = CString::new("flyMain()").unwrap();
      js_eval(ptr, cfilename.as_ptr(), cscript.as_ptr());
      ptr
    };

    rt
  }

  pub fn eval(&self, filename: &str, code: &str) {
    debug!("evaluating '{}'", filename);
    let cfilename = CString::new(filename).unwrap();
    let ccode = CString::new(code).unwrap();
    let ptr = self.ptr;
    self.spawn(future::lazy(move || unsafe {
      js_eval(ptr.0, cfilename.as_ptr(), ccode.as_ptr());
      debug!("finished evaluating '{}'", cfilename.to_string_lossy());
      Ok(())
    }));
  }

  pub fn eval_file(&self, filename: &str) {
    // -> Result<(), tokio::executor::SpawnError> {
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    self.eval(filename, contents.as_str())
  }

  pub fn heap_statistics(&self) -> js_heap_stats {
    unsafe { js_runtime_heap_statistics(self.ptr.0) }
  }

  pub unsafe fn from_raw<'a>(raw: *const js_runtime) -> &'a mut Self {
    let ptr = js_get_data(raw) as *mut _;
    &mut *ptr
  }

  pub fn dispose(&self) {
    unsafe {
      js_runtime_dispose(self.ptr.0);
      Box::from_raw(js_get_data(self.ptr.0) as *mut Runtime); // to drop it
    };
  }

  pub fn run(&mut self) -> oneshot::Receiver<()> {
    self.ready_ch.take().unwrap().send(()).unwrap(); //TODO: no unwrap
                                                     // debug!("self.ready_ch = {:?}", self.ready_ch);
    self.quit_ch.take().unwrap()
  }

  pub fn spawn<F>(&self, fut: F)
  where
    F: Future<Item = (), Error = ()> + Send + 'static,
  {
    let n = NEXT_FUTURE_ID.fetch_add(1, Ordering::SeqCst);
    trace!("SPAWNING A FUTURE! id: {}", n);
    self
      .event_loop
      .lock()
      .unwrap()
      .spawn(fut.then(move |_| Ok(trace!("SPAWNED FUTURE IS DONE id: {}", n))))
      .unwrap();
  }
}

#[cfg(debug_assertions)]
lazy_static! {
  static ref V8ENV_SNAPSHOT: Box<[u8]> = {
    let filename = "v8env/dist/v8env.js";
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let snap = unsafe {
      let cfilename = CString::new(filename).unwrap();
      let ccontents = CString::new(contents).unwrap();
      js_create_snapshot(cfilename.as_ptr(), ccontents.as_ptr())
    };
    let bytes: Vec<u8> =
      unsafe { slice::from_raw_parts(snap.ptr as *const u8, snap.len as usize) }.to_vec();
    bytes.into_boxed_slice()
  };
  pub static ref EVENT_LOOP: (tokio::runtime::TaskExecutor, oneshot::Sender<()>) = {
    let el = tokio::runtime::Runtime::new().unwrap();
    let exec = el.executor();
    let (tx, rx) = oneshot::channel::<()>();
    thread::Builder::new()
      .name("main-event-loop".to_string())
      .spawn(move || {
        el.block_on_all(rx).unwrap();
      }).unwrap();
    (exec, tx)
  };
  static ref HTTP_CLIENT: Client<HttpsConnector<HttpConnector>, Body> = {
    Client::builder()
      .executor(EVENT_LOOP.0.clone())
      .build(HttpsConnector::new(4).unwrap())
  };
}

lazy_static_include_bytes!(V8ENV_SOURCEMAP, "v8env/dist/v8env.js.map");
#[cfg(not(debug_assertions))]
const V8ENV_SNAPSHOT: &'static [u8] = include_bytes!("../v8env.bin");

extern crate sourcemap;
use self::sourcemap::SourceMap;

type SourceMapId = (u32, u32, String, String);

lazy_static! {
  static ref FLY_SNAPSHOT: fly_simple_buf = fly_simple_buf {
    ptr: V8ENV_SNAPSHOT.as_ptr() as *const i8,
    len: V8ENV_SNAPSHOT.len() as i32
  };
  static ref SM_CHAN: Mutex<stdmspc::Sender<(Vec<SourceMapId>, oneshot::Sender<Vec<SourceMapId>>)>> = {
    let (sender, receiver) =
      stdmspc::channel::<(Vec<SourceMapId>, oneshot::Sender<Vec<SourceMapId>>)>();
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
                if filename == "v8env/dist/v8env.js" {
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
  static ref GENERIC_EVENT_LOOP: tokio::runtime::Runtime = {
    let el = tokio::runtime::Runtime::new().unwrap();
    el
  };
}

// Buf represents a byte array returned from a "Op".
// The message might be empty (which will be translated into a null object on
// the javascript side) or it is a heap allocated opaque sequence of bytes.
// Usually a flatbuffer message.
pub type Buf = Option<Box<[u8]>>;

// JS promises in Fly map onto a specific Future
// which yields either a FlyError or a byte array.
pub type Op = Future<Item = Buf, Error = FlyError> + Send;
pub type Handler = fn(JsRuntime, &msg::Base, fly_buf) -> Box<Op>;

use std::slice;

pub extern "C" fn msg_from_js(raw: *const js_runtime, buf: fly_buf, raw_buf: fly_buf) {
  let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
  let base = msg::get_root_as_base(bytes);
  let msg_type = base.msg_type();
  debug!("MSG TYPE: {:?}", msg_type);
  let cmd_id = base.cmd_id();
  // println!("msg id {}", cmd_id);
  let handler: Handler = match msg_type {
    msg::Any::TimerStart => op_timer_start,
    msg::Any::TimerClear => op_timer_clear,
    msg::Any::HttpRequest => op_http_request,
    msg::Any::HttpResponse => op_http_response,
    msg::Any::StreamChunk => op_stream_chunk,
    msg::Any::CacheGet => ops::cache::op_cache_get,
    msg::Any::CacheSet => ops::cache::op_cache_set,
    msg::Any::CacheDel => ops::cache::op_cache_del,
    msg::Any::CacheExpire => ops::cache::op_cache_expire,
    msg::Any::CryptoDigest => op_crypto_digest,
    msg::Any::CryptoRandomValues => op_crypto_random_values,
    msg::Any::SourceMap => op_source_map,
    msg::Any::DataPut => op_data_put,
    msg::Any::DataGet => op_data_get,
    msg::Any::DataDel => op_data_del,
    msg::Any::DataDropCollection => op_data_drop_coll,
    msg::Any::DnsQuery => ops::dns::op_dns_query,
    msg::Any::DnsResponse => ops::dns::op_dns_response,
    msg::Any::AddEventListener => op_add_event_ln,
    msg::Any::LoadModule => op_load_module,
    _ => unimplemented!(),
  };

  let ptr = JsRuntime(raw);

  let fut = handler(ptr, &base, raw_buf);
  let fut = fut.or_else(move |err| {
    error!("error in {:?}: {:?}", msg_type, err);
    Ok(build_error(cmd_id, err))
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
    // debug!("DOING ASYNC MSG");
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
      ptr.send(buf, None);
      Ok(())
    });
    let rt = ptr.to_runtime();
    rt.spawn(fut);
  }
}

pub unsafe extern "C" fn print_from_js(raw: *const js_runtime, lvl: i8, msg: *const libc::c_char) {
  let rt = Runtime::from_raw(raw);
  let msg = CStr::from_ptr(msg).to_string_lossy().into_owned();

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

fn op_timer_start(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  debug!("op_timer_start");
  let msg = base.msg_as_timer_start().unwrap();
  let cmd_id = base.cmd_id();
  let timer_id = msg.id();
  let delay = msg.delay();

  let rt = ptr.to_runtime();

  let timers = &rt.timers;

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

fn remove_timer(ptr: JsRuntime, timer_id: u32) {
  let rt = ptr.to_runtime();
  rt.timers.lock().unwrap().remove(&timer_id);
}

fn op_timer_clear(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_timer_clear().unwrap();
  debug!("op_timer_clear");
  remove_timer(ptr, msg.id());
  ok_future(None)
}

fn op_source_map(_ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_source_map().unwrap();

  let msg_frames = msg.frames().unwrap();
  let mut frames = Vec::with_capacity(msg_frames.len());

  for i in 0..msg_frames.len() {
    let f = msg_frames.get(i);

    debug!(
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

  let (tx, rx) = oneshot::channel::<Vec<SourceMapId>>();
  if let Err(err) = SM_CHAN.lock().unwrap().send((frames, tx)) {
    return odd_future(format!("{}", err).into());
  }

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

fn op_crypto_random_values(_ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
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

fn op_crypto_digest(_ptr: JsRuntime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
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

fn op_add_event_ln(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_add_event_listener().unwrap();

  match msg.event() {
    msg::EventType::Fetch => {
      let (tx, rx) = mpsc::unbounded::<JsHttpRequest>();
      let rt = ptr.to_runtime();
      rt.spawn(
        rx.map_err(|_| error!("error event receiving http request"))
          .for_each(move |req| {
            let builder = &mut FlatBufferBuilder::new();

            let req_url = builder.create_string(req.url.as_str());

            let req_method = match req.method {
              Method::GET => msg::HttpMethod::Get,
              Method::POST => msg::HttpMethod::Post,
              _ => unimplemented!(),
            };

            let headers: Vec<_> = req
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

            let req_headers = builder.create_vector(&headers);

            let req_msg = msg::HttpRequest::create(
              builder,
              &msg::HttpRequestArgs {
                id: req.id,
                method: req_method,
                url: Some(req_url),
                headers: Some(req_headers),
                has_body: req.body.is_some(),
                ..Default::default()
              },
            );

            let to_send = fly_buf_from(
              serialize_response(
                0,
                builder,
                msg::BaseArgs {
                  msg: Some(req_msg.as_union_value()),
                  msg_type: msg::Any::HttpRequest,
                  ..Default::default()
                },
              ).unwrap(),
            );

            ptr.send(to_send, None);

            if let Some(stream) = req.body {
              send_body_stream(ptr, req.id, stream);
            }

            Ok(())
          }),
      );
      rt.fetch_events = Some(tx);
    }
    msg::EventType::Resolv => {
      let (tx, rx) = mpsc::unbounded::<ops::dns::JsDnsRequest>();
      let rt = ptr.to_runtime();
      rt.spawn(
        rx.map_err(|_| error!("error event receiving http request"))
          .for_each(move |req| {
            let builder = &mut FlatBufferBuilder::new();

            let queries: Vec<_> = req
              .queries
              .iter()
              .map(|q| {
                debug!("query: {:?}", q);
                use self::dns::rr::{DNSClass, Name, RecordType};
                let name = builder.create_string(&Name::from(q.name().clone()).to_utf8());
                let rr_type = match q.query_type() {
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
                let dns_class = match q.query_class() {
                  DNSClass::IN => msg::DnsClass::IN,
                  DNSClass::CH => msg::DnsClass::CH,
                  DNSClass::HS => msg::DnsClass::HS,
                  DNSClass::NONE => msg::DnsClass::NONE,
                  DNSClass::ANY => msg::DnsClass::ANY,
                  _ => unimplemented!(),
                };

                msg::DnsQuery::create(
                  builder,
                  &msg::DnsQueryArgs {
                    name: Some(name),
                    rr_type: rr_type,
                    dns_class: dns_class,
                    ..Default::default()
                  },
                )
              }).collect();

            let req_queries = builder.create_vector(&queries);

            let req_msg = msg::DnsRequest::create(
              builder,
              &msg::DnsRequestArgs {
                id: req.id,
                message_type: match req.message_type {
                  dns::op::MessageType::Query => msg::DnsMessageType::Query,
                  _ => unimplemented!(),
                },
                queries: Some(req_queries),
                ..Default::default()
              },
            );

            let to_send = fly_buf_from(
              serialize_response(
                0,
                builder,
                msg::BaseArgs {
                  msg: Some(req_msg.as_union_value()),
                  msg_type: msg::Any::DnsRequest,
                  ..Default::default()
                },
              ).unwrap(),
            );

            ptr.send(to_send, None);
            Ok(())
          }),
      );
      rt.resolv_events = Some(tx);
    }
  };

  ok_future(None)
}

fn send_done_stream(ptr: JsRuntime, req_id: u32) {
  let builder = &mut FlatBufferBuilder::new();
  let chunk_msg = msg::StreamChunk::create(
    builder,
    &msg::StreamChunkArgs {
      id: req_id,
      done: true,
    },
  );
  ptr.send(
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
    None,
  );
}

fn send_stream_chunk(ptr: JsRuntime, req_id: u32, chunk: *mut u8, len: usize, done: bool) {
  let builder = &mut FlatBufferBuilder::new();
  let chunk_msg = msg::StreamChunk::create(
    builder,
    &msg::StreamChunkArgs {
      id: req_id,
      done: done,
    },
  );
  ptr.send(
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
    Some(fly_buf {
      alloc_ptr: ptr::null_mut() as *mut u8,
      alloc_len: 0,
      data_ptr: chunk,
      data_len: len,
    }),
  );
}

fn send_body_stream(ptr: JsRuntime, req_id: u32, stream: JsBody) {
  let rt = ptr.to_runtime();

  match stream {
    JsBody::Static(v) => {
      rt.spawn(future::lazy(move || {
        send_stream_chunk(ptr, req_id, v.as_ptr() as *mut u8, v.len(), true);
        Ok(())
      }));
    }
    JsBody::Stream(rx) => {
      rt.spawn(
        rx.map_err(move |e| error!("error reading from stream channel: {:?}", e))
          .for_each(move |v| {
            send_stream_chunk(ptr, req_id, v.as_ptr() as *mut u8, v.len(), false);
            Ok(())
          }).and_then(move |_| {
            send_done_stream(ptr, req_id);
            Ok(())
          }),
      );
    }
    JsBody::BytesStream(rx) => {
      rt.spawn(
        rx.map_err(move |e| error!("error reading from stream channel: {:?}", e))
          .for_each(move |mut b| {
            send_stream_chunk(ptr, req_id, b.as_mut_ptr() as *mut u8, b.len(), false);
            Ok(())
          }).and_then(move |_| {
            send_done_stream(ptr, req_id);
            Ok(())
          }),
      );
    }
    JsBody::HyperBody(b) => {
      rt.spawn(
        b.map_err(|e| error!("error in hyper body stream read: {:?}", e))
          .for_each(move |chunk| {
            let bytes = chunk.into_bytes();
            send_stream_chunk(
              ptr,
              req_id,
              (*bytes).as_ptr() as *mut u8,
              bytes.len(),
              false,
            );
            Ok(())
          }).and_then(move |_| {
            send_done_stream(ptr, req_id);
            Ok(())
          }),
      );
    }
  };
}

fn op_file_request(ptr: JsRuntime, cmd_id: u32, url: &str) -> Box<Op> {
  let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

  let path: String = url.chars().skip(7).collect();

  let meta = match fs::metadata(path.clone()) {
    Ok(m) => m,
    Err(e) => return odd_future(e.into()),
  };

  println!("META: {:?}", meta);

  if meta.is_file() {
    EVENT_LOOP.0.spawn(future::lazy(move || {
      tokio::fs::File::open(path).then(
        move |fileerr: Result<tokio::fs::File, io::Error>| -> Result<(), ()> {
          debug!("file opened? {}", fileerr.is_ok());
          if let Err(err) = fileerr {
            if p.send(Err(err.into())).is_err() {
              error!("error sending file open error");
            }
            return Ok(());
          }

          let file = fileerr.unwrap(); // should be safe.

          let (tx, rx) = mpsc::unbounded::<BytesMut>();

          if p
            .send(Ok(JsHttpResponse {
              headers: HeaderMap::new(),
              status: StatusCode::OK,
              body: Some(JsBody::BytesStream(rx)),
            })).is_err()
          {
            error!("error sending http response");
            return Ok(());
          }

          EVENT_LOOP.0.spawn(
            FramedRead::new(file, BytesCodec::new())
              .map_err(|e| println!("error reading file chunk! {}", e))
              .for_each(move |chunk| {
                debug!("got frame chunk");
                match tx.clone().unbounded_send(chunk) {
                  Ok(_) => Ok(()),
                  Err(e) => {
                    error!("error sending chunk in channel: {}", e);
                    Err(())
                  }
                }
              }),
          );
          Ok(())
        },
      )
    }));
  } else {
    EVENT_LOOP.0.spawn(future::lazy(move || {
      tokio::fs::read_dir(path).then(move |read_dir_err| {
        if let Err(err) = read_dir_err {
          if p.send(Err(err.into())).is_err() {
            error!("error sending read_dir error");
          }
          return Ok(());
        }
        let read_dir = read_dir_err.unwrap();
        let (tx, rx) = mpsc::unbounded::<Vec<u8>>();

        if p
          .send(Ok(JsHttpResponse {
            headers: HeaderMap::new(),
            status: StatusCode::OK,
            body: Some(JsBody::Stream(rx)),
          })).is_err()
        {
          error!("error sending http response");
          return Ok(());
        }

        EVENT_LOOP.0.spawn(
          read_dir
            .map_err(|e| println!("error read_dir stream: {}", e))
            .for_each(move |entry| {
              let entrypath = entry.path();
              let pathstr = format!("{}\n", entrypath.to_str().unwrap());
              match tx.clone().unbounded_send(pathstr.into()) {
                Ok(_) => Ok(()),
                Err(e) => {
                  error!("error sending path chunk in channel: {}", e);
                  Err(())
                }
              }
            }),
        );
        Ok(())
      })
    }));
  }

  let fut = c
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
          has_body: res.body.is_some(),
          ..Default::default()
        },
      );
      if let Some(stream) = res.body {
        send_body_stream(ptr, req_id, stream);
      }
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

  Box::new(fut)
}

fn op_http_request(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_http_request().unwrap();

  let url = msg.url().unwrap();
  if url.starts_with("file://") {
    return op_file_request(ptr, cmd_id, url);
  }

  let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let req_body: Body;
  if msg.has_body() {
    unimplemented!(); // TODO: implement
  } else {
    req_body = Body::empty();
  }

  let mut req = Request::new(req_body);
  {
    let uri: hyper::Uri = url.parse().unwrap();
    *req.uri_mut() = uri;
    *req.method_mut() = match msg.method() {
      msg::HttpMethod::Get => Method::GET,
      msg::HttpMethod::Head => Method::HEAD,
      msg::HttpMethod::Post => Method::POST,
      _ => unimplemented!(),
    };

    let msg_headers = msg.headers().unwrap();
    let headers = req.headers_mut();
    for i in 0..msg_headers.len() {
      let h = msg_headers.get(i);
      trace!("header: {} => {}", h.key().unwrap(), h.value().unwrap());
      headers.insert(
        HeaderName::from_bytes(h.key().unwrap().as_bytes()).unwrap(),
        h.value().unwrap().parse().unwrap(),
      );
    }
  }

  let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

  let rt = ptr.to_runtime();

  rt.spawn(HTTP_CLIENT.request(req).then(move |reserr| {
    debug!("got http response (or error)");
    if let Err(err) = reserr {
      if p.send(Err(err.into())).is_err() {
        error!("error sending error for http response :/");
      }
      return Ok(());
    }

    let res = reserr.unwrap(); // should be safe.

    let (parts, body) = res.into_parts();

    let mut stream_rx: Option<JsBody> = None;
    let has_body = !body.is_end_stream();
    if has_body {
      stream_rx = Some(JsBody::HyperBody(body));
    }

    if p
      .send(Ok(JsHttpResponse {
        headers: parts.headers,
        status: parts.status,
        body: stream_rx,
      })).is_err()
    {
      error!("error sending http response");
      return Ok(());
    }
    debug!("done with http request");
    Ok(())
  }));

  let fut = c
    .map_err(|e| {
      FlyError::from(io::Error::new(
        io::ErrorKind::Other,
        format!("err getting response from oneshot: {}", e).as_str(),
      ))
    }).and_then(move |reserr: FlyResult<JsHttpResponse>| {
      debug!("IN HTTP RESPONSE RECEIVING END");
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
          has_body: res.body.is_some(),
          ..Default::default()
        },
      );
      if let Some(stream) = res.body {
        send_body_stream(ptr, req_id, stream);
      }
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

  Box::new(fut)
}

fn op_http_response(ptr: JsRuntime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  debug!("handling http response");
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

  let rt = ptr.to_runtime();

  let mut body: Option<JsBody> = None;
  let has_body = msg.has_body();
  if has_body {
    if raw.data_len == 0 {
      debug!("http response will have a streaming body");
      let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
      {
        let mut streams = rt.streams.lock().unwrap();
        streams.insert(req_id, sender);
      }
      body = Some(JsBody::Stream(recver));
    } else {
      body = Some(JsBody::Static(
        unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec(),
      ));
    }
  }

  let mut responses = rt.responses.lock().unwrap();
  match responses.remove(&req_id) {
    Some(sender) => {
      if sender
        .send(JsHttpResponse {
          headers: headers,
          status: status,
          body: body,
        }).is_err()
      {
        return odd_future("error sending http response".to_string().into());
      }
    }
    None => return odd_future("no response receiver!".to_string().into()),
  };

  ok_future(None)
}

fn op_stream_chunk(ptr: JsRuntime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
  debug!("handle stream chunk {:?}", raw);
  let msg = base.msg_as_stream_chunk().unwrap();
  let stream_id = msg.id();

  let rt = ptr.to_runtime();

  let mut streams = rt.streams.lock().unwrap();
  if raw.data_len > 0 {
    match streams.get_mut(&stream_id) {
      Some(sender) => {
        let bytes = unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec();
        match sender.unbounded_send(bytes.to_vec()) {
          Err(e) => error!("error sending chunk: {}", e),
          _ => debug!("chunk streamed"),
        }
      }
      _ => unimplemented!(),
    };
  }
  if msg.done() {
    streams.remove(&stream_id);
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

fn op_data_put(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_data_put().unwrap();
  let coll = msg.collection().unwrap().to_string();
  let key = msg.key().unwrap().to_string();
  let value = msg.json().unwrap().to_string();

  let rt = ptr.to_runtime();

  Box::new(
    rt.data_store
      .put(coll, key, value)
      .map_err(|e| format!("{:?}", e).into())
      .and_then(move |_| Ok(None)),
  )
}

fn op_data_get(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_data_get().unwrap();
  let coll = msg.collection().unwrap().to_string();
  let key = msg.key().unwrap().to_string();

  let rt = ptr.to_runtime();

  Box::new(
    rt.data_store
      .get(coll, key)
      .map_err(|e| format!("error in data store get: {:?}", e).into())
      .and_then(move |s| match s {
        None => Ok(None),
        Some(s) => {
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
      }),
  )
}

fn op_data_del(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_data_del().unwrap();
  let coll = msg.collection().unwrap().to_string();
  let key = msg.key().unwrap().to_string();

  let rt = ptr.to_runtime();

  Box::new(
    rt.data_store
      .del(coll, key)
      .map_err(|e| format!("{:?}", e).into())
      .and_then(move |_| Ok(None)),
  )
}

fn op_data_drop_coll(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_data_drop_collection().unwrap();
  let coll = msg.collection().unwrap().to_string();

  let rt = ptr.to_runtime();

  Box::new(
    rt.data_store
      .drop_coll(coll)
      .map_err(|e| format!("{:?}", e).into())
      .and_then(move |_| Ok(None)),
  )
}

fn op_load_module(_ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_load_module().unwrap();
  let module_specifier = msg.module_specifier().unwrap().to_string();
  let containing_file = msg.containing_file().unwrap().to_string();

  let module =
    match ::compiler::Compiler::new(None).fetch_module(&module_specifier, &containing_file) {
      Ok(m) => m,
      Err(e) => return odd_future(e.into()),
    };

  Box::new(future::lazy(move || {
    let builder = &mut FlatBufferBuilder::new();
    let module_id = builder.create_string(&module.module_id);
    let file_name = builder.create_string(&module.file_name);
    let source_code = builder.create_string(&module.source_code);

    let msg = msg::LoadModuleResp::create(
      builder,
      &msg::LoadModuleRespArgs {
        module_id: Some(module_id),
        file_name: Some(file_name),
        source_code: Some(source_code),
      },
    );
    Ok(serialize_response(
      cmd_id,
      builder,
      msg::BaseArgs {
        msg: Some(msg.as_union_value()),
        msg_type: msg::Any::LoadModuleResp,
        ..Default::default()
      },
    ))
  }))
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_runtime_dispose() {}
}
