use slog::{Drain, Level, Logger};
use std::env;
use std::panic::{RefUnwindSafe, UnwindSafe};

pub fn build_logger<D: slog::Drain<Err = slog::Never, Ok = ()> + Send + 'static>(
    drain: D,
) -> Logger {
    slog::Logger::root(slog_async::Async::default(drain.fuse()).fuse(), slog::o!())
}

pub fn build_routing_logger<
    D1: slog::Drain<Err = slog::Never, Ok = ()> + Send + 'static,
    D2: slog::Drain<Err = slog::Never, Ok = ()> + Send + 'static,
>(
    runtime_drain: D1,
    app_drain: D2,
) -> Logger {
    slog::Logger::root(
        slog_async::Async::default(
            SwitchDrain {
                runtime_drain,
                app_drain,
            }
            .fuse(),
        )
        .fuse(),
        slog::o!(),
    )
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

pub struct SwitchDrain<D1: slog::Drain, D2: slog::Drain> {
    runtime_drain: D1,
    app_drain: D2,
}

impl<D1: slog::Drain, D2: slog::Drain> UnwindSafe for SwitchDrain<D1, D2> {}
impl<D1: slog::Drain, D2: slog::Drain> RefUnwindSafe for SwitchDrain<D1, D2> {}

impl<'a, D1: slog::Drain, D2: slog::Drain> slog::Drain for SwitchDrain<D1, D2> {
    type Err = slog::Never;
    type Ok = ();

    fn log(
        &self,
        record: &slog::Record,
        values: &slog::OwnedKVList,
    ) -> Result<Self::Ok, Self::Err> {
        if record.tag() == "app" {
            self.app_drain.log(record, values).ok();
        } else {
            self.runtime_drain.log(record, values).ok();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slog::slog_info;
    use tempfile;

    #[test]
    fn test_separate_drains() {
        let runtime_buf = tempfile::NamedTempFile::new().unwrap();
        let app_buf = tempfile::NamedTempFile::new().unwrap();

        {
            let logger = build_routing_logger(
                slog_json::Json::default(runtime_buf.reopen().unwrap()).fuse(),
                slog_json::Json::default(app_buf.reopen().unwrap()).fuse(),
            );

            slog_info!(logger, #"runtime", "runtime-message");
            slog_info!(logger, #"app", "app-message");
        }

        let runtime_output = std::fs::read_to_string(runtime_buf.path()).unwrap();
        let app_output = std::fs::read_to_string(app_buf.path()).unwrap();

        assert!(
            runtime_output.contains("runtime-message"),
            "runtime message not written to runtime drain!"
        );
        assert!(
            !runtime_output.contains("app-message"),
            "app message written to runtime drain!"
        );

        assert!(
            app_output.contains("app-message"),
            "app message not written to app drain!"
        );
        assert!(
            !app_output.contains("runtime-message"),
            "runtime message written to app drain!"
        );
    }

    #[test]
    fn test_single_drain() {
        let buf = tempfile::NamedTempFile::new().unwrap();

        {
            let logger = build_logger(slog_json::Json::default(buf.reopen().unwrap()).fuse());

            slog_info!(logger, #"runtime", "runtime-message");
            slog_info!(logger, #"app", "app-message");
        }

        let output = std::fs::read_to_string(buf.path()).unwrap();
        assert!(
            output.contains("runtime-message"),
            "runtime message not written to drain!"
        );
        assert!(
            output.contains("app-message"),
            "runtime message not written to drain!"
        );
    }
}
