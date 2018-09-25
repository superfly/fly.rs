extern crate r2d2_redis;

use self::r2d2_redis::{r2d2, redis, RedisConnectionManager};
use self::redis::Commands;
use futures::future::FutureResult;
use futures::{future, stream};
use std::ops::Deref;

use std::sync::Arc;

lazy_static! {
  pub static ref REDIS_CACHE_POOL: Arc<r2d2::Pool<RedisConnectionManager>> = {
    let manager = RedisConnectionManager::new("redis://localhost").unwrap();
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    Arc::new(pool)
  };
}

// TODO: make async
pub fn redis_stream(
  key: String,
) -> Box<stream::Stream<Item = Vec<u8>, Error = redis::RedisError> + Send> {
  let pool = Arc::clone(&REDIS_CACHE_POOL);
  let con = pool.get().unwrap(); // TODO: no unwrap
  let size = 256 * 1024;
  Box::new(stream::unfold(0, move |pos| {
    // println!("unfolding... pos: {}, modulo: {}", pos, pos % size);

    // End early given some rules!
    // not a multiple of size, means we're done.
    if pos > 0 && pos % size > 0 {
      return None;
    }
    match redis::cmd("GETRANGE")
      .arg(key.clone())
      .arg(pos)
      .arg(pos + size - 1) // end arg is inclusive
      .query::<Vec<u8>>(con.deref())
    {
      Ok(r) => {
        let len = r.len();
        if (len == 0) {
          return None;
        }
        Some(future::ok::<(Vec<u8>, usize), _>((r, pos + len)))
      }
      Err(e) => Some(future::err(e)),
    }
  }))
}
