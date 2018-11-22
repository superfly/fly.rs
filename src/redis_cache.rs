use std::sync::Arc;

use futures::{future, stream, Future, Stream};
use std::ops::Deref;

use cache_store::*;

extern crate r2d2_redis;
use self::r2d2_redis::{r2d2, redis, RedisConnectionManager};

use settings::RedisStoreConfig;

#[derive(Debug)]
pub struct RedisCacheStore {
  pool: Arc<r2d2::Pool<RedisConnectionManager>>,
  ns: Option<String>,
}

impl RedisCacheStore {
  pub fn new(conf: &RedisStoreConfig) -> Self {
    let manager = RedisConnectionManager::new(conf.url.as_str()).unwrap();
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    RedisCacheStore {
      pool: Arc::new(pool),
      ns: conf.namespace.as_ref().cloned(),
    }
  }

  fn key(&self, k: String) -> String {
    if self.ns.is_none() {
      return k;
    }
    format!("{}:{}", self.ns.as_ref().unwrap(), k)
  }
}

impl CacheStore for RedisCacheStore {
  fn get(
    &self,
    key: String,
  ) -> CacheResult<Option<Box<Stream<Item = Vec<u8>, Error = CacheError> + Send>>> {
    let pool = Arc::clone(&self.pool);
    let conn = pool.get().unwrap(); // TODO: no unwrap
    let size = 256 * 1024;
    let fullkey = self.key(key);
    debug!("redis cache get with key: {}", fullkey);
    Ok(Some(Box::new(stream::unfold(0, move |pos| {
      // End early given some rules!
      // not a multiple of size, means we're done.
      if pos > 0 && pos % size > 0 {
        return None;
      }
      match redis::cmd("GETRANGE")
        .arg(fullkey.clone())
        .arg(pos)
        .arg(pos + size - 1) // end arg is inclusive
        .query::<Vec<u8>>(conn.deref())
      {
        Ok(r) => {
          let len = r.len();
          if len == 0 {
            return None;
          }
          Some(future::ok::<(Vec<u8>, usize), _>((r, pos + len)))
        }
        Err(e) => Some(future::err(CacheError::Failure(format!("{}", e)))),
      }
    }))))
  }

  fn set(
    &self,
    key: String,
    data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
    maybe_ttl: Option<u32>,
  ) -> Box<Future<Item = (), Error = CacheError> + Send> {
    let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!(
      "redis cache set with key: {} and ttl: {:?}",
      fullkey, maybe_ttl
    );
    Box::new(
      data_stream
        .concat2()
        .map_err(|_e| {
          error!("redis cache set error concatenating stream");
          CacheError::Unknown
        }).and_then(move |b| {
          let conn = pool.get().unwrap(); // TODO: no unwrap
          let mut cmd = redis::cmd("SET");
          cmd.arg(fullkey).arg(b);
          if let Some(ttl) = maybe_ttl {
            cmd.arg("EX").arg(ttl);
          }
          match cmd.query::<String>(conn.deref()) {
            Ok(_) => Ok(()),
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
          }
        }),
    )
  }

  fn del(&self, key: String) -> Box<Future<Item = (), Error = CacheError> + Send> {
    let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!("redis cache del key: {}", fullkey);
    Box::new(future::lazy(move || -> Result<(), CacheError> {
      let conn = pool.get().unwrap(); // TODO: no unwrap
      match redis::cmd("DEL").arg(fullkey).query::<i8>(conn.deref()) {
        Ok(_) => Ok(()),
        Err(e) => Err(CacheError::Failure(format!("{}", e))),
      }
    }))
  }

  fn expire(&self, key: String, ttl: u32) -> Box<Future<Item = (), Error = CacheError> + Send> {
    let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!("redis cache expire key: {} w/ ttl: {}", fullkey, ttl);

    Box::new(future::lazy(move || -> CacheResult<()> {
      let conn = pool.get().unwrap(); // TODO: no unwrap
      match redis::cmd("EXPIRE")
        .arg(fullkey)
        .arg(ttl)
        .query::<i8>(conn.deref())
      {
        Ok(_) => Ok(()),
        Err(e) => Err(CacheError::Failure(format!("{}", e))),
      }
    }))
  }
}
