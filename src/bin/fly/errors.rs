use std;
use std::fmt;
use std::io;

pub type FlyCliResult<T> = Result<T, FlyCliError>;

#[derive(Debug)]
pub struct FlyCliError {
  repr: Repr,
}

#[derive(Debug)]
enum Repr {
  Simple(String),
  IoErr(io::Error),
  ClapError(clap::Error),
}

pub fn new(msg: String) -> FlyCliError {
  FlyCliError {
    repr: Repr::Simple(msg),
  }
}

impl fmt::Display for FlyCliError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self.repr {
      Repr::IoErr(ref err) => err.fmt(f),
      Repr::Simple(ref s) => write!(f, "{}", s),
      Repr::ClapError(ref err) => err.fmt(f),
    }
  }
}

impl std::error::Error for FlyCliError {
  fn description(&self) -> &str {
    match self.repr {
      Repr::IoErr(ref err) => err.description(),
      Repr::Simple(ref s) => s.as_str(),
      Repr::ClapError(ref err) => err.description(),
    }
  }

  fn cause(&self) -> Option<&std::error::Error> {
    match self.repr {
      Repr::IoErr(ref err) => Some(err),
      Repr::Simple(_) => None,
      Repr::ClapError(ref err) => Some(err),
    }
  }
}

impl From<io::Error> for FlyCliError {
  #[inline]
  fn from(err: io::Error) -> FlyCliError {
    FlyCliError {
      repr: Repr::IoErr(err),
    }
  }
}

impl From<clap::Error> for FlyCliError {
  #[inline]
  fn from(err: clap::Error) -> FlyCliError {
    FlyCliError {
      repr: Repr::ClapError(err),
    }
  }
}

impl From<String> for FlyCliError {
  #[inline]
  fn from(err: String) -> FlyCliError {
    FlyCliError {
      repr: Repr::Simple(err),
    }
  }
}

impl From<globset::Error> for FlyCliError {
  fn from(err: globset::Error) -> FlyCliError {
    new(format!("Invalid path pattern: {}", err))
  }
}

impl From<()> for FlyCliError {
  #[inline]
  fn from(_: ()) -> FlyCliError {
    FlyCliError {
      repr: Repr::Simple("Errored with non message error.".to_string()),
    }
  }
}
