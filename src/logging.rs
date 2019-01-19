use slog::{o, Drain, Logger};
use slog_scope::GlobalLoggerGuard;

pub fn configure() -> (GlobalLoggerGuard, Logger) {
  let decorator = slog_term::TermDecorator::new().build();
  let drain = slog_term::FullFormat::new(decorator).build().fuse();
  let drain = slog_async::Async::new(drain).build().fuse();
  let logger = slog::Logger::root(drain, o!("build" => crate::build_number()));
  let runtime_logger = logger.new(o!("source" => "rt"));
  let app_logger = logger.new(o!("source" => "app"));
  let _guard = slog_scope::set_global_logger(runtime_logger);
  slog_stdlog::init().unwrap();

  return (_guard, app_logger);
}
