use slog::{o, Drain, Level, Logger};
use slog_async::Async;
use slog_scope::GlobalLoggerGuard;
use slog_term::term_full;
use std::env;

pub fn configure() -> (GlobalLoggerGuard, Logger) {
  let runtime_logger = Logger::root(
    Async::default(
      term_full()
        .filter_level(log_level_from_env("FLY_LOG_LEVEL", Level::Warning))
        .fuse(),
    )
    .fuse(),
    o!(
      "build" => crate::build_number(),
      "source" => "rt",
    ),
  );

  let app_logger = Logger::root(
    Async::default(
      term_full()
        .filter_level(log_level_from_env("LOG_LEVEL", Level::Debug))
        .fuse(),
    )
    .fuse(),
    o!(
      "build" => crate::build_number(),
      "source" => "app",
    ),
  );

  let _guard = slog_scope::set_global_logger(runtime_logger);
  slog_stdlog::init().unwrap();

  return (_guard, app_logger);
}

pub fn log_level_from_env(name: &str, default: Level) -> Level {
  match env::var(name).unwrap_or_default().as_ref() {
    "TRACE" => Level::Trace,
    "DEBUG" => Level::Debug,
    "INFO" => Level::Info,
    "WARN" => Level::Warning,
    "WARNING" => Level::Warning,
    "ERROR" => Level::Error,
    _ => default,
  }
}
