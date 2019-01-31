use slog::{o, Drain, Level, Logger};
use slog_async::Async;
use slog_scope::GlobalLoggerGuard;
use slog_term::term_full;
use std::env;

pub fn configure() -> (GlobalLoggerGuard, Logger) {
    let root_logger = Logger::root(
        Async::default(term_full().fuse()).fuse(),
        o!(
          "build" => crate::BUILD_VERSION,
        ),
    );

    let runtime_logger = Logger::root(
        root_logger
            .new(o!())
            .filter_level(log_level_from_env("FLY_LOG_LEVEL", Level::Warning))
            .fuse(),
        o!("source" => "rt"),
    );

    let app_logger = Logger::root(
        root_logger
            .filter_level(log_level_from_env("LOG_LEVEL", Level::Debug))
            .fuse(),
        o!("source" => "app"),
    );

    let _guard = slog_scope::set_global_logger(runtime_logger);
    slog_stdlog::init().unwrap();

    return (_guard, app_logger);
}

pub fn log_level_from_env(name: &str, default: Level) -> Level {
    match env::var(name).unwrap_or_default().to_uppercase().as_str() {
        "TRACE" => Level::Trace,
        "DEBUG" => Level::Debug,
        "INFO" => Level::Info,
        "WARN" | "WARNING" => Level::Warning,
        "ERROR" => Level::Error,
        _ => default,
    }
}
