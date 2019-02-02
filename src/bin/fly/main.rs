mod commands;
mod errors;
mod util;

#[macro_use]
extern crate log;
use crate::errors::FlyCliResult;
use crate::util::*;
use clap::AppSettings;
use slog::{Drain, Level};

fn main() -> FlyCliResult<()> {
  let logger = build_logger();
  let _guard = slog_scope::set_global_logger(logger);
  slog_stdlog::init().unwrap();

  let args = cli().get_matches();
  let (cmd, subcommand_args) = args.subcommand();
  let exec_fn = commands::command_exec(cmd).expect("Unknown command");

  exec_fn(subcommand_args.unwrap())
}

fn cli() -> App {
  App::new("fly")
    .global_settings(&[
      AppSettings::DeriveDisplayOrder,
      AppSettings::DontCollapseArgsInUsage,
      AppSettings::ArgRequiredElseHelp,
      AppSettings::UnifiedHelpMessage,
    ])
    .about("Edge application runtime")
    .subcommands(commands::commands())
}

fn build_logger() -> slog::Logger {
  fly::logging::build_routing_logger(
    slog_term::term_full()
      .filter_level(fly::logging::log_level_from_env(
        "FLY_LOG_LEVEL",
        Level::Warning,
      ))
      .fuse(),
    slog_term::term_full()
      .filter_level(fly::logging::log_level_from_env("LOG_LEVEL", Level::Info))
      .fuse(),
  )
}
