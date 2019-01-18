extern crate http;
extern crate libc;

use tokio;

use tokio::runtime::current_thread;

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

use std::sync::RwLock;

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

use self::hyper::rt::{Future, Stream};
use self::hyper::HeaderMap;
use self::hyper::{Body, Method, StatusCode};

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::errors::FlyError;

extern crate log;

extern crate rand;
use self::rand::{thread_rng, Rng};

extern crate bytes;
use self::bytes::BytesMut;

use crate::acme_store;
use crate::cache_store;
use crate::data_store;
use crate::fs_store;
use crate::ops;
use crate::utils::*;

use crate::postgres_data;
use crate::redis_acme;
use crate::redis_cache;
use crate::sqlite_cache;
use crate::sqlite_data;

use crate::{disk_fs, redis_fs};

use crate::settings::{
  AcmeStoreConfig, CacheStore, CacheStoreNotifier, DataStore, FsStore, Settings,
};

use crate::module_resolver::{
  LoadedModule, LocalDiskModuleResolver, ModuleResolver, ModuleResolverManager, RefererInfo,
  StandardModuleResolverManager,
};

use super::NEXT_FUTURE_ID;
use std::net::SocketAddr;
use std::str;

use std::time;

extern crate trust_dns as dns;

pub enum JsBody {
  BoxedStream(Box<Stream<Item = Vec<u8>, Error = FlyError> + Send>),
  Stream(mpsc::UnboundedReceiver<Vec<u8>>),
  BytesStream(mpsc::UnboundedReceiver<BytesMut>),
  HyperBody(Body),
  Static(Vec<u8>),
}

pub struct JsHttpResponse {
  pub headers: HeaderMap,
  pub status: StatusCode,
  pub body: Option<JsBody>,
}

pub struct JsHttpRequest {
  pub id: u32,
  pub method: http::Method,
  pub remote_addr: SocketAddr,
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
  pub version: String,
  pub event_loop: Mutex<current_thread::Handle>,
  timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
  pub responses: Mutex<HashMap<u32, oneshot::Sender<JsHttpResponse>>>,
  pub dns_responses: Mutex<HashMap<u32, oneshot::Sender<ops::dns::JsDnsResponse>>>,
  pub streams: Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>,
  pub cache_store: Box<cache_store::CacheStore + 'static + Send + Sync>,
  pub data_store: Box<data_store::DataStore + 'static + Send + Sync>,
  pub fs_store: Box<fs_store::FsStore + 'static + Send + Sync>,
  pub acme_store: Option<Box<acme_store::AcmeStore + 'static + Send + Sync>>,
  pub fetch_events: Option<mpsc::UnboundedSender<JsHttpRequest>>,
  pub resolv_events: Option<mpsc::UnboundedSender<ops::dns::JsDnsRequest>>,
  pub last_event_at: AtomicUsize,
  pub module_resolver_manager: Box<ModuleResolverManager>,
  metadata_cache: RwLock<HashMap<i32, Box<LoadedModule>>>,
  ready_ch: Option<oneshot::Sender<()>>,
  quit_ch: Option<oneshot::Receiver<()>>,
}

static JSINIT: Once = Once::new();

fn init_event_loop(
  name: String,
) -> (
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
    .name(format!("runtime-loop-{}", name))
    .spawn(move || {
      let mut el = current_thread::Runtime::new().unwrap();
      let (txready, rxready) = oneshot::channel::<()>();
      let (txquit, rxquit) = oneshot::channel::<()>();

      c.send((el.handle(), txready, rxquit)).unwrap();

      // keep it alive at least until all scripts are evaled
      el.spawn(
        rxready
          .map_err(|_| error!("error recving ready signal for runtime"))
          .and_then(|_| Ok(warn!("ready ch received!"))),
      );

      match el.run() {
        Ok(_) => warn!("runtime event loop ran fine"),
        Err(e) => error!("error running runtime event loop: {}", e),
      };
      warn!("Event loop has run its course.");
      match txquit.send(()) {
        Ok(_) => warn!("Sent quit () in channel successfully."),
        Err(_) => error!("error sending quit signal for runtime"),
      };
    })
    .unwrap();
  p.wait().unwrap()
}

impl Runtime {
  pub fn new(
    name: Option<String>,
    version: Option<String>,
    settings: &Settings,
    module_resolvers: Option<Vec<Box<ModuleResolver>>>,
  ) -> Box<Runtime> {
    JSINIT.call_once(|| unsafe { js_init() });

    let rt_name = name.unwrap_or("v8".to_string());
    let rt_version = version.unwrap_or("0".to_string());
    let (rthandle, txready, rxquit) = init_event_loop(format!("{}-{}", rt_name, rt_version));
    let rt_module_resolvers =
      module_resolvers.unwrap_or(vec![
        Box::new(LocalDiskModuleResolver::new(None)) as Box<ModuleResolver>
      ]);

    let mut rt = Box::new(Runtime {
      ptr: JsRuntime(ptr::null() as *const js_runtime),
      name: rt_name,
      version: rt_version,
      event_loop: Mutex::new(rthandle.clone()),
      ready_ch: Some(txready),
      quit_ch: Some(rxquit),
      timers: Mutex::new(HashMap::new()),
      responses: Mutex::new(HashMap::new()),
      dns_responses: Mutex::new(HashMap::new()),
      streams: Mutex::new(HashMap::new()),
      // stream_recv: Mutex::new(HashMap::new()),
      fetch_events: None,
      resolv_events: None,
      cache_store: match settings.cache_store {
        Some(ref store) => match store {
          CacheStore::Sqlite(conf) => {
            Box::new(sqlite_cache::SqliteCacheStore::new(conf.filename.clone()))
          }
          CacheStore::Redis(conf) => Box::new(redis_cache::RedisCacheStore::new(
            &conf,
            match settings.cache_store_notifier {
              None => None,
              Some(CacheStoreNotifier::Redis(ref csnconf)) => Some(csnconf.clone()),
            },
          )),
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
      fs_store: match settings.fs_store {
        Some(ref store) => match store {
          FsStore::Redis(conf) => Box::new(redis_fs::RedisFsStore::new(&conf)),
          FsStore::Disk => Box::new(disk_fs::DiskFsStore::new()),
        },
        None => Box::new(disk_fs::DiskFsStore::new()),
      },
      acme_store: match settings.acme_store {
        Some(ref config) => match config {
          AcmeStoreConfig::Redis(config) => {
            Some(Box::new(redis_acme::RedisAcmeStore::new(&config)))
          }
        },
        None => None,
      },
      last_event_at: ATOMIC_USIZE_INIT,
      module_resolver_manager: Box::new(StandardModuleResolverManager::new(
        rt_module_resolvers,
        None,
      )),
      metadata_cache: RwLock::new(HashMap::new()),
    });

    (*rt).ptr.0 = unsafe {
      let ptr = js_runtime_new(js_runtime_options {
        snapshot: *FLY_SNAPSHOT,
        data: rt.as_ref() as *const _ as *mut libc::c_void,
        recv_cb: msg_from_js,
        print_cb: print_from_js,
        resolve_cb: resolve_callback,
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
    unsafe {
      js_eval(ptr.0, cfilename.as_ptr(), ccode.as_ptr());
    }
    debug!("finished evaluating '{}'", cfilename.to_string_lossy());
  }

  pub fn eval_file(&self, filename: &str) {
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

  pub fn dispose(&mut self) {
    // stop listening to events
    self.fetch_events.take();
    self.resolv_events.take();

    match self.timers.lock() {
      Ok(mut timers) => timers.clear(),
      Err(_) => error!("error acquiring lock to clear timers"),
    };

    match self.streams.lock() {
      Ok(mut streams) => streams.clear(),
      Err(_) => error!("error acquiring lock to clear streams"),
    };

    unsafe {
      js_runtime_dispose(self.ptr.0);
    };
  }

  pub fn run(&mut self) -> oneshot::Receiver<()> {
    self.ready_ch.take().unwrap().send(()).unwrap(); //TODO: no unwrap
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

  pub fn dispatch_event(
    &self,
    id: u32,
    event: JsEvent,
  ) -> Option<Result<EventResponseChannel, EventDispatchError>> {
    let res = match event {
      JsEvent::Fetch(req) => match self.fetch_events {
        None => return None,
        Some(ref ch) => match self.responses.lock() {
          Ok(mut guard) => {
            let (tx, rx) = oneshot::channel::<JsHttpResponse>();
            guard.insert(id, tx);
            match ch.unbounded_send(req) {
              Ok(_) => EventResponseChannel::Http(rx),
              Err(e) => return Some(Err(EventDispatchError::Http(e))),
            }
          }
          Err(_) => return Some(Err(EventDispatchError::PoisonedLock)),
        },
      },
      JsEvent::Resolv(req) => match self.resolv_events {
        None => return None,
        Some(ref ch) => match self.dns_responses.lock() {
          Ok(mut guard) => {
            let (tx, rx) = oneshot::channel::<ops::dns::JsDnsResponse>();
            guard.insert(id, tx);
            match ch.unbounded_send(req) {
              Ok(_) => EventResponseChannel::Dns(rx),
              Err(e) => return Some(Err(EventDispatchError::Dns(e))),
            }
          }
          Err(_) => return Some(Err(EventDispatchError::PoisonedLock)),
        },
      },
    };

    if let Ok(epoch) = time::SystemTime::now().duration_since(time::UNIX_EPOCH) {
      self
        .last_event_at
        .store(epoch.as_secs() as usize, Ordering::SeqCst);
    }

    Some(Ok(res))
  }

  pub fn get_module_metadata(&self, hash: &i32) -> Option<Box<LoadedModule>> {
    return match self.metadata_cache.read().unwrap().get(hash) {
      Some(v) => Some((*v).clone()),
      None => None,
    };
  }

  pub fn insert_module_metadata(&mut self, hash: i32, module_metadata: LoadedModule) {
    let locked_cache = self.metadata_cache.get_mut().unwrap();
    if locked_cache.contains_key(&hash) {
      error!("Attempted to overwrite entry in module metadata cache.");
    } else {
      locked_cache.insert(hash, Box::new(module_metadata));
    }
  }
}

pub enum JsEvent {
  Fetch(JsHttpRequest),
  Resolv(ops::dns::JsDnsRequest),
}

pub enum EventResponseChannel {
  Http(oneshot::Receiver<JsHttpResponse>),
  Dns(oneshot::Receiver<ops::dns::JsDnsResponse>),
}

#[derive(Debug)]
pub enum EventDispatchError {
  PoisonedLock,
  Http(mpsc::SendError<JsHttpRequest>),
  Dns(mpsc::SendError<ops::dns::JsDnsRequest>),
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
              })
              .collect(),
          )
          .unwrap();
        }
      })
      .unwrap();
    Mutex::new(sender)
  };
  static ref GENERIC_EVENT_LOOP: tokio::runtime::Runtime = {
    let el = tokio::runtime::Runtime::new().unwrap();
    el
  };
  pub static ref EVENT_LOOP: (tokio::runtime::TaskExecutor, oneshot::Sender<()>) = {
    let el = tokio::runtime::Runtime::new().unwrap();
    let exec = el.executor();
    let (tx, rx) = oneshot::channel::<()>();
    thread::Builder::new()
      .name("main-event-loop".to_string())
      .spawn(move || {
        el.block_on_all(rx).unwrap();
      })
      .unwrap();
    (exec, tx)
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
  let handler: Handler = match msg_type {
    msg::Any::TimerStart => op_timer_start,
    msg::Any::TimerClear => op_timer_clear,
    msg::Any::HttpRequest => ops::fetch::op_fetch,
    msg::Any::HttpResponse => ops::fetch::op_http_response,
    msg::Any::StreamChunk => op_stream_chunk,
    msg::Any::CacheGet => ops::cache::op_cache_get,
    msg::Any::CacheSet => ops::cache::op_cache_set,
    msg::Any::CacheDel => ops::cache::op_cache_del,
    msg::Any::CacheNotifyDel => ops::cache::op_cache_notify_del,
    msg::Any::CacheNotifyPurgeTag => ops::cache::op_cache_notify_purge_tag,
    msg::Any::CacheExpire => ops::cache::op_cache_expire,
    msg::Any::CacheSetMeta => ops::cache::op_cache_set_meta,
    msg::Any::CachePurgeTag => ops::cache::op_cache_purge_tag,
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
    msg::Any::ImageApplyTransforms => ops::image::op_image_transform,
    msg::Any::AcmeGetChallenge => ops::acme::op_get_challenge,
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
            )
            .unwrap(),
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

pub unsafe extern "C" fn resolve_callback(
  raw: *const js_runtime,
  specifier: *const libc::c_char,
  referer_identity_hash: i32,
) -> js_compiled_module {
  let rt = Runtime::from_raw(raw);
  let specifier_str = CStr::from_ptr(specifier).to_string_lossy().into_owned();

  let referer_loaded_module = match rt.get_module_metadata(&referer_identity_hash) {
    Some(v) => v,
    None => {
      error!("Failed to find module hash in metadata cache! Exiting.");
      std::process::exit(1);
    }
  };

  let loaded_module = match rt.module_resolver_manager.resolve_module(
    specifier_str,
    Some(RefererInfo {
      origin_url: referer_loaded_module.origin_url,
      is_wasm: Some(referer_loaded_module.loaded_source.is_wasm),
      source_code: Some(referer_loaded_module.loaded_source.source),
      indentifier_hash: Some(referer_identity_hash),
    }),
  ) {
    Ok(v) => v,
    Err(e) => {
      error!("Failed to resolve and load module! Exiting. {}", e);
      std::process::exit(1);
    }
  };

  let module_data = js_module_data {
    origin_url: CString::new(loaded_module.origin_url).unwrap().as_ptr(),
    source_map_url: CString::new("").unwrap().as_ptr(),
    is_wasm: loaded_module.loaded_source.is_wasm,
    source_code: fly_simple_buf {
      ptr: CString::new(loaded_module.loaded_source.source.as_str())
        .unwrap()
        .as_ptr(),
      len: loaded_module.loaded_source.source.len() as i32,
    },
  };

  let compile_result = js_compile_module(raw, module_data);

  if compile_result.success {
    return compile_result.compiled_module;
  } else {
    error!("Module compile failed! Exiting.");
    std::process::exit(1);
  }
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

    let filename = match f.filename() {
      Some(f) => f,
      None => "",
    };

    let line = f.line();
    let col = f.col();

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
          })
          .collect();
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
              Method::HEAD => msg::HttpMethod::Head,
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
              })
              .collect();

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
              )
              .unwrap(),
            );

            ptr.send(to_send, None);

            if let Some(stream) = req.body {
              send_body_stream(ptr, req.id, stream);
            }

            Ok(())
          })
          .and_then(|_| Ok(info!("done listening to http events."))),
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
              })
              .collect();

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
              )
              .unwrap(),
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
      None => unimplemented!(),
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
    })
    .select(cancel_rx)
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
  let rt = _ptr.to_runtime();
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_load_module().unwrap();
  let specifier_url = msg.specifier_url().unwrap().to_string();

  let referer_info = match msg.referer_origin_url() {
    Some(v) => Some(RefererInfo {
      origin_url: v.to_string(),
      is_wasm: Some(false),
      source_code: None,
      indentifier_hash: None,
    }),
    None => None,
  };

  let module = match rt
    .module_resolver_manager
    .resolve_module(specifier_url, referer_info)
  {
    Ok(m) => m,
    Err(e) => return odd_future(e.into()),
  };

  Box::new(future::lazy(move || {
    let builder = &mut FlatBufferBuilder::new();
    let origin_url = builder.create_string(&module.origin_url);
    let source_code = builder.create_string(&module.loaded_source.source);

    let msg = msg::LoadModuleResp::create(
      builder,
      &msg::LoadModuleRespArgs {
        origin_url: Some(origin_url),
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
