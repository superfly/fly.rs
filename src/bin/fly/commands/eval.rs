use crate::errors::*;
use crate::util::*;
use clap::{Arg, ArgMatches};
use fly::runtime::{Runtime, RuntimeConfig};
use fly::settings::SETTINGS;
use futures::Future;

pub fn cli() -> App {
    subcommand("eval").about("Run a file").arg(
        Arg::with_name("input")
            .help("the input file to use")
            .required(true)
            .index(1),
    )
}

pub fn exec(args: &ArgMatches<'_>) -> FlyCliResult<()> {
    let mut runtime = Runtime::new(RuntimeConfig {
        name: None,
        version: None,
        settings: &SETTINGS.read().unwrap(),
        module_resolvers: None,
        app_logger: &slog_scope::logger(),
        msg_handler: None,
        permissions: None,
        dev_tools: true,
    });

    let entry_file = args.value_of("input").unwrap();
    runtime.eval_file_with_dev_tools(entry_file);
    runtime.run().wait().unwrap();

    Ok(())
}
