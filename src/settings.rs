extern crate config;
use self::config::{Config, ConfigError, Environment, File};
use std::sync::RwLock;

lazy_static! {
  pub static ref SETTINGS: RwLock<Settings> = RwLock::new(Settings::new().unwrap());
}

#[derive(Debug, Deserialize)]
pub struct SqliteStoreConfig {
  pub filename: String,
}

#[derive(Debug, Deserialize)]
pub struct PostgresStoreConfig {
  pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct RedisStoreConfig {
  pub url: String,
}

#[derive(Debug, Deserialize)]
pub enum DataStore {
  Sqlite(SqliteStoreConfig),
  Postgres(PostgresStoreConfig),
}

#[derive(Debug, Deserialize)]
pub enum CacheStore {
  Sqlite(SqliteStoreConfig),
  Redis(RedisStoreConfig),
}

#[derive(Debug, Deserialize)]
pub struct Settings {
  pub data_store: Option<DataStore>,
  pub cache_store: Option<CacheStore>,
}

impl Settings {
  pub fn new() -> Result<Self, ConfigError> {
    let mut s = Config::new();

    s.merge(File::with_name(".fly").required(false))?;
    s.merge(Environment::with_prefix("FLY"))?;
    s.try_into()
  }
}
