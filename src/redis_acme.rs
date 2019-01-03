use crate::acme_challenge::*;
use futures::{future, Future};
use crate::settings::RedisStoreConfig;
use std::fmt::Display;
extern crate r2d2_redis;
use self::r2d2_redis::RedisConnectionManager;
use self::r2d2_redis::{r2d2, redis};
use redis::Commands;

pub struct RedisAcmeChallengeStore {
  pool: r2d2::Pool<RedisConnectionManager>,
  ns: Option<String>,
}

impl RedisAcmeChallengeStore {
  pub fn new(conf: &RedisStoreConfig) -> Self {
    RedisAcmeChallengeStore {
      pool: r2d2::Pool::new(RedisConnectionManager::new(conf.url.as_str()).unwrap()).unwrap(),
      ns: conf.namespace.as_ref().cloned(),
    }
  }

  fn hostname_key<S: Display>(&self, hostname: S) -> String {
    format!("{}:hostname:{}:certificate:challenges", self.ns.as_ref().unwrap_or(&"".to_string()), hostname)
  }
}

impl AcmeChallengeStore for RedisAcmeChallengeStore {
  fn check_token(
    &self,
    hostname: String,
    token: String,
  ) -> Box<Future<Item = bool, Error = AcmeChallengeError> + Send> {
        let fullkey = self.hostname_key(hostname);
        debug!("redis acme get with key: {}", fullkey);

        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(AcmeChallengeError::Failure(format!("{}", e))),
            Ok(conn) => match redis::cmd("ZREVRANGE").arg(&fullkey).arg(0).arg(10).query::<Vec<String>>(&*conn) {
              Err(e) => Err(AcmeChallengeError::Failure(format!("{}", e))),
              Ok(tokens) => {
                for pending_token in &tokens {
                  if pending_token.starts_with(&token) {
                    return Ok(true);
                  }
                }
                Ok(false)
              }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
  use super::*;
  extern crate rand;
  use std::time;

  fn setup() -> RedisAcmeChallengeStore {
    RedisAcmeChallengeStore::new(&RedisStoreConfig {
      url: "redis://localhost:6379".to_string(),
      namespace: Some("test:acme-challenge".to_string()),
    })
  }

  fn set_token(
    store: &RedisAcmeChallengeStore,
    hostname: &str,
    token: &str,
  ) {
    let conn = store.pool.get().unwrap();
    let key = store.hostname_key(&hostname);
    let ts = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let challenge_contents = format!("{}.{}", token, ts);

    let _: i32 = conn.zadd(key.to_string(), challenge_contents.to_string(), ts).expect("Failed to set token");
  }

  #[test]
  fn test_check_token() {
    let store = setup();

    set_token(&store, "fly.io", "valid-token");

    let cases = [
        ("fly.io", "valid-token", true),
        ("fly.io", "missing-token", false),
        ("edgeapp.net", "valid-token", false),
    ];

    for &test in cases.iter() {
        assert_eq!(test.2, store.check_token(test.0.to_string(), test.1.to_string()).wait().unwrap());
    }
  }
}
