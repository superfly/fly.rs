use tokio;

use tokio::runtime::current_thread;

use std::ffi::{CStr, CString};
use std::sync::{Mutex, Once};

use self::fs::File;
use std::fs;
use std::io::Read;

use libfly::*;

use futures::{
  sync::{mpsc, oneshot},
  Future,
};
use std::collections::HashMap;

use std::thread;

use std::sync::RwLock;

use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

use std::ptr;
use std::slice;

use crate::js::*;

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::errors::FlyError;

use slog::{slog_debug, slog_error, slog_info, slog_o, slog_trace, slog_warn, Logger};

use crate::acme_store;
use crate::cache_store;
use crate::data_store;
use crate::fs_store;
use crate::utils::*;

use crate::postgres_data;
use crate::redis_acme;
use crate::redis_cache;
use crate::sqlite_cache;
use crate::sqlite_data;

use crate::{disk_fs, redis_fs};

use crate::v8env::{DEV_TOOLS_SOURCE, FLY_SNAPSHOT};

use crate::runtime_permissions::RuntimePermissions;
use crate::settings::{
  AcmeStoreConfig, CacheStore, CacheStoreNotifier, DataStore, FsStore, Settings,
};

use crate::module_resolver::{
  LoadedModule, LocalDiskModuleResolver, ModuleResolver, ModuleResolverManager, RefererInfo,
  StandardModuleResolverManager,
};

use super::NEXT_FUTURE_ID;
use std::str;

use std::time;

use crate::msg_handler::{DefaultMessageHandler, MessageHandler};

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
  pub app_logger: Logger,
  pub event_loop: Mutex<current_thread::Handle>,
  pub timers: Mutex<HashMap<u32, oneshot::Sender<()>>>,
  pub responses: Mutex<HashMap<u32, oneshot::Sender<JsHttpResponse>>>,
  pub dns_responses: Mutex<HashMap<u32, oneshot::Sender<JsDnsResponse>>>,
  pub streams: Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>,
  pub cache_store: Box<cache_store::CacheStore + 'static + Send + Sync>,
  pub data_store: Box<data_store::DataStore + 'static + Send + Sync>,
  pub fs_store: Box<fs_store::FsStore + 'static + Send + Sync>,
  pub acme_store: Option<Box<acme_store::AcmeStore + 'static + Send + Sync>>,
  pub fetch_events: Option<mpsc::UnboundedSender<JsHttpRequest>>,
  pub resolv_events: Option<mpsc::UnboundedSender<JsDnsRequest>>,
  pub last_event_at: AtomicUsize,
  pub module_resolver_manager: Box<ModuleResolverManager>,
  pub msg_handler: Box<MessageHandler>,
  pub permissions: RuntimePermissions,
  pub dev_tools: bool,
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
          .and_then(|_| Ok(trace!("ready ch received!"))),
      );

      match el.run() {
        Ok(_) => trace!("runtime event loop ran fine"),
        Err(e) => error!("error running runtime event loop: {}", e),
      };
      trace!("Event loop has run its course.");
      match txquit.send(()) {
        Ok(_) => trace!("Sent quit () in channel successfully."),
        Err(_) => error!("error sending quit signal for runtime"),
      };
    })
    .unwrap();
  p.wait().unwrap()
}

pub struct RuntimeConfig<'a> {
  pub name: Option<String>,
  pub version: Option<String>,
  pub settings: &'a Settings,
  pub module_resolvers: Option<Vec<Box<ModuleResolver>>>,
  pub app_logger: &'a Logger,
  pub msg_handler: Option<Box<MessageHandler>>,
  pub permissions: Option<RuntimePermissions>,
  pub dev_tools: bool,
}

impl Runtime {
  pub fn new(config: RuntimeConfig) -> Box<Runtime> {
    JSINIT.call_once(|| unsafe { js_init() });

    let rt_name = config.name.unwrap_or("v8".to_string());
    let rt_version = config.version.unwrap_or("0".to_string());
    let app_logger = config
      .app_logger
      .new(slog_o!("app_name" => rt_name.to_owned(), "app_version" => rt_version.to_owned()));
    let (rthandle, txready, rxquit) = init_event_loop(format!("{}-{}", rt_name, rt_version));
    let rt_module_resolvers =
      config.module_resolvers.unwrap_or(vec![
        Box::new(LocalDiskModuleResolver::new(None)) as Box<ModuleResolver>
      ]);

    let mut rt = Box::new(Runtime {
      ptr: JsRuntime(ptr::null() as *const js_runtime),
      name: rt_name,
      version: rt_version,
      app_logger: app_logger,
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
      cache_store: match config.settings.cache_store {
        Some(ref store) => match store {
          CacheStore::Sqlite(conf) => {
            Box::new(sqlite_cache::SqliteCacheStore::new(conf.filename.clone()))
          }
          CacheStore::Redis(conf) => Box::new(redis_cache::RedisCacheStore::new(
            &conf,
            match config.settings.cache_store_notifier {
              None => None,
              Some(CacheStoreNotifier::Redis(ref csnconf)) => Some(csnconf.clone()),
            },
          )),
        },
        None => Box::new(sqlite_cache::SqliteCacheStore::new("cache.db".to_string())),
      },
      data_store: match config.settings.data_store {
        Some(ref store) => match store {
          DataStore::Sqlite(conf) => {
            Box::new(sqlite_data::SqliteDataStore::new(conf.filename.clone()))
          }
          DataStore::Postgres(conf) => Box::new(postgres_data::PostgresDataStore::new(&conf)),
        },
        None => Box::new(sqlite_data::SqliteDataStore::new("data.db".to_string())),
      },
      fs_store: match config.settings.fs_store {
        Some(ref store) => match store {
          FsStore::Redis(conf) => Box::new(redis_fs::RedisFsStore::new(&conf)),
          FsStore::Disk => Box::new(disk_fs::DiskFsStore::new()),
        },
        None => Box::new(disk_fs::DiskFsStore::new()),
      },
      acme_store: match config.settings.acme_store {
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
      msg_handler: config
        .msg_handler
        .unwrap_or(Box::new(DefaultMessageHandler {})),
      permissions: config.permissions.unwrap_or_default(),
      dev_tools: config.dev_tools,
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

    if config.dev_tools {
      debug!("Loading dev tools");
      rt.eval("dev-tools.js", *DEV_TOOLS_SOURCE);
      rt.eval("<installDevTools>", "installDevTools();");
      debug!("Loading dev tools done");
    }

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

  pub fn eval_file_with_dev_tools(&self, filename: &str) {
    self.eval(filename, &format!("dev.run('{}')", filename));
  }

  pub fn heap_statistics(&self) -> js_heap_stats {
    unsafe { js_runtime_heap_statistics(self.ptr.0) }
  }

  pub unsafe fn from_raw<'a>(raw: *const js_runtime) -> &'a mut Self {
    let ptr = js_get_data(raw) as *mut _;
    &mut *ptr
  }

  pub fn dispose(&mut self) {
    {
      // stop listening to events
      self.fetch_events.take();
      self.resolv_events.take();
    };

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
            let (tx, rx) = oneshot::channel::<JsDnsResponse>();
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

lazy_static! {
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

pub extern "C" fn msg_from_js(raw: *const js_runtime, buf: fly_buf, raw_buf: fly_buf) {
  let bytes = unsafe { slice::from_raw_parts(buf.data_ptr, buf.data_len) };
  let base = msg::get_root_as_base(bytes);
  let ptr = JsRuntime(raw);
  let rt = ptr.to_runtime();

  let msg_type = base.msg_type();
  let cmd_id = base.cmd_id();

  let fut = rt
    .msg_handler
    .handle_msg(ptr.to_runtime(), &base, raw_buf)
    .or_else(move |err| {
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

  match lvl {
    // print to STDOUT, no logging
    0 => println!("{}", msg),

    // runtime messages from logger
    1 => slog_error!(rt.app_logger, #"runtime", "{}", msg; "source" => "v8env"),
    2 => slog_warn!(rt.app_logger, #"runtime", "{}", msg; "source" => "v8env"),
    3 => slog_info!(rt.app_logger, #"runtime", "{}", msg; "source" => "v8env"),
    4 => slog_debug!(rt.app_logger, #"runtime", "{}", msg; "source" => "v8env"),
    5 => slog_trace!(rt.app_logger, #"runtime", "{}", msg; "source" => "v8env"),

    // app messages from console
    11 => slog_error!(rt.app_logger, #"app", "{}", msg; "source" => "app"),
    12 => slog_warn!(rt.app_logger, #"app", "{}", msg; "source" => "app"),
    13 => slog_info!(rt.app_logger, #"app", "{}", msg; "source" => "app"),
    14 => slog_debug!(rt.app_logger, #"app", "{}", msg; "source" => "app"),
    15 => slog_trace!(rt.app_logger, #"app", "{}", msg; "source" => "app"),

    _ => slog_info!(rt.app_logger, #"runtime", "{}", msg; "source" => "v8env"),
  };
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
