use crate::errors::*;
use crate::util::*;
use clap::{Arg, ArgMatches};
use fly::runtime::{Runtime, RuntimeConfig};
use fly::runtime_permissions::RuntimePermissions;
use fly::settings::SETTINGS;
use futures::Future;

const PATTERN_DEFAULT: &str = "**/*.{test,spec}.{js,ts}";

pub fn cli() -> App {
    subcommand("test")
        .about("Run unit tests")
        .arg(
            Arg::with_name("paths")
                .help("Paths or patterns for test files to run.")
                .default_value(PATTERN_DEFAULT)
                .multiple(true)
                .index(1),
        )
        .arg(
            clap::Arg::with_name("lib")
                .short("l")
                .long("lib")
                .help("Libraries or shims to load before app code")
                .takes_value(true)
                .multiple(true),
        )
}

pub fn exec(args: &ArgMatches<'_>) -> FlyCliResult<()> {
    let mut rt = Runtime::new(RuntimeConfig {
        name: None,
        version: None,
        settings: &SETTINGS.read().unwrap(),
        module_resolvers: None,
        app_logger: &slog_scope::logger(),
        msg_handler: None,
        permissions: Some(RuntimePermissions::new(true)),
        dev_tools: true,
    });

    if args.is_present("lib") {
        for lib_path in glob(args.values_of("lib").unwrap().collect(), None)? {
            rt.eval_file(&lib_path);
        }
    }

    let test_files = glob(args.values_of("paths").unwrap().collect(), None)?;

    rt.eval(
        "<runTests>",
        &format!(
            "dev.runTests({});",
            serde_json::to_string(&test_files).expect("error loading test files")
        ),
    );

    tokio::run(
        rt.run()
            .map_err(|_| error!("error running runtime event loop")),
    );

    Ok(())
}
