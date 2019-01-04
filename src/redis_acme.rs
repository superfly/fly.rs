use crate::acme_store::*;
use crate::settings::RedisStoreConfig;
use futures::{future, Future};
use std::fmt::Display;
extern crate r2d2_redis;
use self::r2d2_redis::RedisConnectionManager;
use self::r2d2_redis::{r2d2, redis};

pub struct RedisAcmeStore {
  pool: r2d2::Pool<RedisConnectionManager>,
  ns: Option<String>,
}

impl RedisAcmeStore {
  pub fn new(conf: &RedisStoreConfig) -> Self {
    RedisAcmeStore {
      pool: r2d2::Pool::new(RedisConnectionManager::new(conf.url.as_str()).unwrap()).unwrap(),
      ns: conf.namespace.as_ref().cloned(),
    }
  }

  fn hostname_key<S: Display>(&self, hostname: S) -> String {
    format!(
      "acme-challenge:{}:hostname:{}",
      self.ns.as_ref().unwrap_or(&"".to_string()),
      hostname
    )
  }
}

impl AcmeStore for RedisAcmeStore {
  fn validate_challenge(
    &self,
    hostname: String,
    token: String,
  ) -> Box<Future<Item = Option<String>, Error = AcmeError> + Send> {
    let fullkey = self.hostname_key(hostname);
    info!("redis acme get with key: {}", fullkey);

    let pool = self.pool.clone();
    Box::new(future::lazy(move || match pool.get() {
      Err(e) => Err(AcmeError::Failure(format!("{}", e))),
      Ok(conn) => match redis::cmd("ZREVRANGE")
        .arg(&fullkey)
        .arg(0)
        .arg(10)
        .query::<Vec<String>>(&*conn)
      {
        Err(e) => Err(AcmeError::Failure(format!("{}", e))),
        Ok(tokens) => {
          for pending_token in &tokens {
            info!(
              "acme token: '{}'.starts_with('{}') == {}",
              pending_token,
              &token,
              pending_token.starts_with(&token)
            );
            if pending_token.starts_with(&token) {
              return Ok(Some(pending_token.to_string()));
            }
          }
          Ok(None)
        }
      },
    }))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  extern crate rand;
  use redis::Commands;
  use std::time;

  fn setup() -> RedisAcmeStore {
    RedisAcmeStore::new(&RedisStoreConfig {
      url: "redis://localhost:6379".to_string(),
      namespace: Some("test:acme-challenge".to_string()),
    })
  }

  fn set_token(store: &RedisAcmeStore, hostname: &str, contents: &str) {
    let conn = store.pool.get().unwrap();
    let key = store.hostname_key(&hostname);
    let ts = time::SystemTime::now()
      .duration_since(time::UNIX_EPOCH)
      .unwrap()
      .as_secs();

    let _: i32 = conn
      .zadd(key.to_string(), contents.to_string(), ts)
      .expect("Failed to set token");
  }

  #[test]
  fn test_validate_challenge() {
    let store = setup();

    set_token(&store, "fly.io", "valid-token.abc123");

    let cases = [
      (
        "fly.io",
        "valid-token",
        Some("valid-token.abc123".to_owned()),
      ),
      ("fly.io", "missing-token", None),
      ("edgeapp.net", "valid-token", None),
    ];

    for test in cases.iter() {
      let actual = store
        .validate_challenge(test.0.to_string(), test.1.to_string())
        .wait()
        .unwrap();
      assert_eq!(actual, test.2);
    }
  }
}
