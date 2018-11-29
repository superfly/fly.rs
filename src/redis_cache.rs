use futures::{future, stream, Future, Stream};

use crate::cache_store::*;

extern crate redis;
use self::redis::Commands;

use crate::settings::RedisStoreConfig;

#[derive(Debug)]
pub struct RedisCacheStore {
  client: redis::Client,
  ns: Option<String>,
}

impl RedisCacheStore {
  pub fn new(conf: &RedisStoreConfig) -> Self {
    let client = redis::Client::open(conf.url.as_str()).unwrap();
    RedisCacheStore {
      client,
      ns: conf.namespace.as_ref().cloned(),
    }
  }

  fn key(&self, k: String) -> String {
    if let Some(ref ns) = self.ns {
      return format!("{}:{}", ns, k);
    }
    k
  }

  fn tag_key(&self, t: String) -> String {
    if let Some(ref ns) = self.ns {
      return format!("tag:{}:{}", ns, t);
    }
    t
  }
}

impl CacheStore for RedisCacheStore {
  fn get(
    &self,
    key: String,
  ) -> CacheResult<Option<Box<Stream<Item = Vec<u8>, Error = CacheError> + Send>>> {
    let size = 256 * 1024;
    let fullkey = self.key(key);
    debug!("redis cache get with key: {}", fullkey);
    let client = self.client;
    Ok(Some(Box::new(stream::unfold(0, move |pos| {
      if pos > 0 && pos % size > 0 {
        return None;
      }

      match client.clone().get_connection() {
        Ok(conn) => {
          match redis::cmd("GETRANGE")
            .arg(&fullkey)
            .arg(pos)
            .arg(pos + size - 1) // end arg is inclusive
            .query::<Vec<u8>>(&conn)
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
        }
        Err(e) => Some(future::err(CacheError::Failure(format!("{}", e)))),
      }
    }))))
  }

  fn set(
    &self,
    key: String,
    data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
    opts: CacheSetOptions,
  ) -> Box<Future<Item = (), Error = CacheError> + Send> {
    // let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!(
      "redis cache set with key: {} and ttl: {:?}",
      fullkey, opts.ttl
    );
    Box::new(
      self
        .client
        .get_async_connection()
        .map_err(|e| CacheError::Failure(format!("{}", e)))
        .and_then(move |conn| {
          data_stream
            .concat2()
            .map_err(|_e| {
              error!("redis cache set error concatenating stream");
              CacheError::Unknown
            }).and_then(move |b| {
              let mut cmd = redis::cmd("SET");
              cmd.arg(&fullkey).arg(b);
              if let Some(ttl) = opts.ttl {
                cmd.arg("EX").arg(ttl);
              }
              cmd
                .query_async(conn)
                .map_err(|e| CacheError::Failure(format!("{}", e)))
                .and_then(|_| Ok(()))
            })
        }),
    )
  }

  fn del(&self, key: String) -> Box<Future<Item = (), Error = CacheError> + Send> {
    // let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!("redis cache del key: {}", fullkey);
    Box::new(
      self
        .client
        .get_async_connection()
        .map_err(|e| CacheError::Failure(format!("{}", e)))
        .and_then(move |conn| {
          redis::cmd("DEL")
            .arg(&fullkey)
            .query_async(conn)
            .map_err(|e| CacheError::Failure(format!("{}", e)))
            .and_then(|_| Ok(()))
        }),
    )
  }

  fn expire(&self, key: String, ttl: u32) -> Box<Future<Item = (), Error = CacheError> + Send> {
    // let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!("redis cache expire key: {} w/ ttl: {}", fullkey, ttl);

    Box::new(
      self
        .client
        .get_async_connection()
        .map_err(|e| CacheError::Failure(format!("{}", e)))
        .and_then(move |conn| {
          redis::cmd("EXPIRE")
            .arg(&fullkey)
            .arg(ttl)
            .query_async(conn)
            .map_err(|e| CacheError::Failure(format!("{}", e)))
            .and_then(|_| Ok(()))
        }),
    )
  }

  fn ttl(&self, key: String) -> Box<Future<Item = i32, Error = CacheError> + Send> {
    // let pool = Arc::clone(&self.pool);
    let fullkey = self.key(key);
    debug!("redis cache ttl key: {}", fullkey);

    Box::new(
      self
        .client
        .get_async_connection()
        .map_err(|e| CacheError::Failure(format!("{}", e)))
        .and_then(move |conn| {
          redis::cmd("TTL")
            .arg(&fullkey)
            .query_async(conn)
            .map_err(|e| CacheError::Failure(format!("{}", e)))
            .and_then(|(_, ttl)| Ok(ttl))
        }),
    )
  }

  fn purge_tag(&self, tag: String) -> Box<Future<Item = (), Error = CacheError> + Send> {
    //   let pool = Arc::clone(&self.pool);
    debug!("redis cache purge_tag tag: {}", tag);
    let tagkey = self.tag_key(tag);

    Box::new(
      self
        .client
        .get_shared_async_connection()
        .map_err(|e| CacheError::Failure(format!("{}", e)))
        .and_then(move |conn| {
          redis::cmd("SMEMBERS")
            .arg(&tagkey)
            .query_async(conn.clone())
            .map_err(|e| CacheError::Failure(format!("{}", e)))
            .and_then(move |(_, members)| {
              let mut cmd = redis::cmd("DEL");
              for key in members {
                cmd.arg(key);
              }
              cmd
                .query_async(conn)
                .map_err(|e| CacheError::Failure(format!("{}", e)))
                .and_then(|(_, ttl)| Ok(ttl))
            })
        }),
    )

    //   Box::new(future::lazy(move || -> CacheResult<()> {
    //     let conn = pool.get().unwrap(); // TODO: no unwrap
    //     match conn.deref().sscan::<Vec<String>>(key) {
    //       Ok(iter) {
    //         // for k in iter {
    //         //   redis::cmd("")
    //         // }
    //         Ok(())
    //       },
    //       Err(e) => Err(CacheError::Failure(format!("{}", e))),
    //     }
    //   })
  }
}
