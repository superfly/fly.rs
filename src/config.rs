extern crate config;
use self::config::{Config, Environment, File};
use std::sync::RwLock;

lazy_static! {
  pub static ref CONFIG: RwLock<Config> = {
    let mut settings = Config::default();
    settings
      .merge(File::with_name(".fly").required(false))
      .unwrap()
      .merge(Environment::with_prefix("FLY"))
      .unwrap();

    RwLock::new(settings)
  };
}
