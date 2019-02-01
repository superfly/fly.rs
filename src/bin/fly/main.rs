mod commands;
mod errors;
mod util;

#[macro_use]
extern crate log;
use crate::errors::FlyCliResult;
use crate::util::*;
use clap::AppSettings;

fn main() -> FlyCliResult<()> {
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
