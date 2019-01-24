use crate::errors::{permission_denied, FlyResult};

#[derive(Debug)]
pub struct RuntimePermissions {
  pub allow_os: bool,
}

impl RuntimePermissions {
  pub fn new(allow_os: bool) -> Self {
    Self { allow_os }
  }

  pub fn check_os(&self) -> FlyResult<()> {
    if self.allow_os {
      Ok(())
    } else {
      Err(permission_denied())
    }
  }
}

impl Default for RuntimePermissions {
  fn default() -> Self {
    RuntimePermissions { allow_os: false }
  }
}
