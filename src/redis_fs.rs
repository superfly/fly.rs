use crate::fs_store::*;
use futures::{future, stream, Future};

use crate::settings::RedisStoreConfig;
use std::fmt::Display;

extern crate r2d2_redis;
use self::r2d2_redis::RedisConnectionManager;
use self::r2d2_redis::{r2d2, redis};

use crate::redis_pool::get_pool;

pub struct RedisFsStore {
    pool: r2d2::Pool<RedisConnectionManager>,
    ns: Option<String>,
}

impl RedisFsStore {
    pub fn new(conf: &RedisStoreConfig) -> Self {
        RedisFsStore {
            pool: get_pool(conf.url.clone()),
            ns: conf.namespace.as_ref().cloned(),
        }
    }

    fn file_key<S: Display>(&self, key: S) -> String {
        format!("{}:{}", self.ns.as_ref().unwrap_or(&"".to_string()), key)
    }
}

impl FsStore for RedisFsStore {
    fn read(&self, path: String) -> Box<Future<Item = Option<FsEntry>, Error = FsError> + Send> {
        let fullkey = self.file_key(path);
        debug!("redis fs get with key: {}", fullkey);

        let pool = self.pool.clone();
        // let getpool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(FsError::Failure(format!("{}", e))),
            Ok(conn) => match redis::cmd("EXISTS").arg(&fullkey).query::<bool>(&*conn) {
                Err(e) => Err(FsError::Failure(format!("{}", e))),
                Ok(exists) => {
                    if !exists {
                        return Ok(None);
                    }
                    let size = 256 * 1024;
                    Ok(Some(FsEntry {
                        stream: Box::new(stream::unfold(0, move |pos| {
                            // End early given some rules!
                            // not a multiple of size, means we're done.
                            if pos > 0 && pos % size > 0 {
                                return None;
                            }
                            match redis::cmd("GETRANGE")
                                .arg(&fullkey)
                                .arg(pos)
                                .arg(pos + size - 1) // end arg is inclusive
                                .query::<Vec<u8>>(&*conn)
                            {
                                Ok(r) => {
                                    let len = r.len();
                                    if len == 0 {
                                        return None;
                                    }
                                    Some(future::ok::<(Vec<u8>, usize), _>((r, pos + len)))
                                }
                                Err(e) => Some(future::err(FsError::Failure(format!("{}", e)))),
                            }
                        })),
                    }))
                }
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate rand;
    use self::rand::{thread_rng, RngCore};

    use futures::Stream;

    #[test]
    fn test_redis_fs_read() {
        let store = RedisFsStore::new(&RedisStoreConfig {
            url: "redis://localhost:6379".to_string(),
            namespace: Some("fstest".to_string()),
        });
        let path = "README.md";

        let mut v = [0u8; 1000];
        thread_rng().fill_bytes(&mut v);

        let conn = store.pool.get().unwrap();
        redis::cmd("SET")
            .arg(store.file_key(path))
            .arg(v.to_vec())
            .query::<()>(&*conn)
            .unwrap();

        assert_eq!(
            store
                .read(path.to_string())
                .wait()
                .unwrap()
                .unwrap()
                .stream
                .concat2()
                .wait()
                .unwrap(),
            v.to_vec()
        );

        assert!(store.read("notfound".to_string()).wait().unwrap().is_none());
    }
}
