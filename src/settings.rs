extern crate config;
use self::config::{Config, ConfigError, Environment, File};
use std::sync::RwLock;

lazy_static! {
    pub static ref SETTINGS: RwLock<Settings> = RwLock::new(Settings::new().unwrap());
}

#[derive(Debug, Deserialize, Clone)]
pub struct SqliteStoreConfig {
    pub filename: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PostgresStoreConfig {
    pub url: String,
    pub database: Option<String>,
    pub tls_client_crt: Option<String>,
    pub tls_client_key: Option<String>,
    pub tls_ca_crt: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisStoreConfig {
    pub url: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisCacheNotifierConfig {
    pub reader_url: String,
    pub writer_url: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FsStore {
    Redis(RedisStoreConfig),
    Disk,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataStore {
    Sqlite(SqliteStoreConfig),
    Postgres(PostgresStoreConfig),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CacheStore {
    Sqlite(SqliteStoreConfig),
    Redis(RedisStoreConfig),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcmeStoreConfig {
    Redis(RedisStoreConfig),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CacheStoreNotifier {
    Redis(RedisCacheNotifierConfig),
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub data_store: Option<DataStore>,
    pub cache_store: Option<CacheStore>,
    pub cache_store_notifier: Option<CacheStoreNotifier>,
    pub fs_store: Option<FsStore>,
    pub acme_store: Option<AcmeStoreConfig>,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();

        s.merge(File::with_name(".fly").required(false))?;
        s.merge(Environment::with_prefix("FLY"))?;
        s.try_into()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            data_store: None,
            cache_store: None,
            cache_store_notifier: None,
            fs_store: None,
            acme_store: None,
        }
    }
}
