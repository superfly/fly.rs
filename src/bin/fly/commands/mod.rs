use crate::util::*;

pub fn commands() -> Vec<App> {
  vec![http::cli(), test::cli(), dns::cli(), eval::cli()]
}

pub fn command_exec(name: &str) -> Option<ExecFn> {
  let exec = match name {
    "dns" => dns::exec,
    "eval" => eval::exec,
    "http" => http::exec,
    "test" => test::exec,
    _ => return None,
  };

  Some(exec)
}

pub mod dns;
pub mod eval;
pub mod http;
pub mod test;
