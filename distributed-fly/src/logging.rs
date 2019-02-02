use slog::{o, Drain, Level};
use slog_json;

use std::env;

pub fn build_logger() -> slog::Logger {
    build_logger_internal(std::io::stdout())
}

fn build_logger_internal<W: std::io::Write + Send + 'static>(io: W) -> slog::Logger {
    let runtime_drain = slog_json::Json::new(io).build().ignore_res();

    let logger = match std::net::TcpStream::connect("localhost:9514") {
        Ok(stream) => {
            println!("connected to tcp");
            let app_drain = slog_json::Json::new(stream).build().ignore_res();
            fly::logging::build_routing_logger(runtime_drain, app_drain)
        }
        Err(e) => {
            println!("did not connect to tcp {} ", e);
            eprintln!(
                "Error connecting to app log endpoint, falling back to stdout: {} ",
                e
            );
            fly::logging::build_logger(runtime_drain)
        }
    };

    logger.new(o!(
        "rt_version" => fly::BUILD_VERSION,
        "message" => slog::PushFnValue(move |record : &slog::Record, ser| {
            ser.emit(record.msg())
        }),
        "level" => slog::FnValue(move |rinfo : &slog::Record| {
            numeric_level(rinfo.level())
        }),
        "timestamp" => slog::PushFnValue(move |_ : &slog::Record, ser| {
            ser.emit(chrono::Utc::now().to_rfc3339())
        }),
        "region" => env::var("REGION").unwrap_or_default(),
        "host" => env::var("HOST").unwrap_or_default(),
    ))
}

fn numeric_level(level: Level) -> u8 {
    match level {
        Level::Critical => 2,
        Level::Error => 3,
        Level::Warning => 4,
        Level::Info => 6,
        Level::Debug => 7,
        Level::Trace => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slog::slog_info;
    use std::process::{Command, Stdio};
    use std::time;
    use tempfile;

    #[test]
    fn test_syslog_available() {
        let tmpio = tempfile::NamedTempFile::new().unwrap();
        let nc = Command::new("nc")
            .args(&["-l", "9514"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start netcat");

        // give netcat a chance to start
        std::thread::sleep(time::Duration::from_millis(100));

        {
            let _logger = build_logger_internal(tmpio.reopen().unwrap());

            slog_info!(_logger, #"runtime", "runtime-message");
            slog_info!(_logger, #"app", "app-message");
        }

        let output = nc.wait_with_output().unwrap();
        let tcp_output = String::from_utf8_lossy(&output.stdout);
        let stdio_output = std::fs::read_to_string(tmpio.path()).unwrap();

        assert!(
            tcp_output.contains("app-message"),
            "app log was not written to app drain!"
        );
        assert!(
            !tcp_output.contains("runtime-message"),
            "runtime message was written to app drain!"
        );

        assert!(
            !stdio_output.contains("app-message"),
            "app log was written to stdout!"
        );
        assert!(
            stdio_output.contains("runtime-message"),
            "runtime message was not written to stdout!"
        );
    }

    #[test]
    fn test_syslog_not_available() {
        let tmpio = tempfile::NamedTempFile::new().unwrap();

        {
            let _logger = build_logger_internal(tmpio.reopen().unwrap());

            slog_info!(_logger, #"runtime", "runtime-message");
            slog_info!(_logger, #"app", "app-message");
        }

        let stdio_output = std::fs::read_to_string(tmpio.path()).unwrap();

        assert!(
            stdio_output.contains("app-message"),
            "app log was not written to stdout!"
        );
        assert!(
            stdio_output.contains("runtime-message"),
            "runtime message was not written to stdout!"
        );
    }
}
