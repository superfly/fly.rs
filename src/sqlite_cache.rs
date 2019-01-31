extern crate r2d2;
extern crate r2d2_sqlite;
extern crate rusqlite;
use self::r2d2_sqlite::SqliteConnectionManager;

use self::rusqlite::types::ToSql;
use self::rusqlite::NO_PARAMS;

use futures::{future, stream, Future, Stream};
use std::ops::Deref;

use std::io::{Read, Seek, SeekFrom};

use crate::cache_store::*;
use crate::cache_store_notifier::{CacheOperation, CacheStoreNotifierError};

impl From<rusqlite::Error> for CacheError {
  #[inline]
  fn from(err: rusqlite::Error) -> CacheError {
    CacheError::Failure(format!("{}", err))
  }
}

pub struct SqliteCacheStore {
  pool: r2d2::Pool<SqliteConnectionManager>,
}

impl SqliteCacheStore {
  pub fn new(filename: String) -> Self {
    let manager = SqliteConnectionManager::file(filename);
    let pool = r2d2::Pool::new(manager).unwrap();
    let con = pool.get().unwrap(); // TODO: no unwrap
    con
      .execute(
        "CREATE TABLE IF NOT EXISTS cache (
      key TEXT PRIMARY KEY NOT NULL,
      value BLOB NOT NULL,
      meta TEXT,
      expires_at DATETIME
    );
    CREATE UNIQUE INDEX IF NOT EXISTS ON cache (key);
    CREATE INDEX IF NOT EXISTS ON cache (key, expires_at);",
        NO_PARAMS,
      )
      .unwrap();

    SqliteCacheStore { pool }
  }
}

impl CacheStore for SqliteCacheStore {
  fn get(&self, key: String) -> Box<Future<Item = Option<CacheEntry>, Error = CacheError> + Send> {
    debug!("sqlite cache get with key: {}", key);
    let pool = self.pool.clone();
    let conn = pool.get().unwrap(); // TODO: no unwrap

    Box::new(future::lazy(move || {
      let size = 256 * 1024;

      let mut stmt = conn
        .prepare(
          "SELECT rowid,meta FROM cache
      WHERE key = ? AND
        (
          expires_at IS NULL OR
          expires_at >= datetime('now')
        ) LIMIT 1",
        )
        .unwrap();

      let mut rows = stmt.query(&[&key])?;

      let row_res = rows.next();

      let rowid: i64 = match row_res {
        Some(Ok(ref row)) => row.get(0),
        Some(Err(e)) => return Err(CacheError::Failure(format!("{}", e))),
        None => {
          debug!("row not found");
          return Ok(None);
        }
      };

      let meta: Option<String> = match row_res {
        Some(Ok(ref row)) => row.get(1),
        Some(Err(e)) => {
          error!("error getting metadata from row: {}", e);
          None
        }
        None => None,
      };

      Ok(Some(CacheEntry {
        meta: meta,
        stream: Box::new(stream::unfold(0, move |pos| {
          debug!("sqlite cache get in stream future, pos: {}", pos);

          // End early given some rules!
          // not a multiple of size, means we're done.
          if pos > 0 && pos % size > 0 {
            return None;
          }

          let conn = pool.get().unwrap();

          let mut blob = match conn.deref().blob_open(
            rusqlite::DatabaseName::Main,
            "cache",
            "value",
            rowid,
            true,
          ) {
            Ok(b) => b,
            Err(e) => return Some(future::err(e.into())),
          };

          if let Err(e) = blob.seek(SeekFrom::Start(pos)) {
            return Some(future::err(e.into()));
          }
          let mut buf = [0u8; 256 * 1024];
          match blob.read(&mut buf[..]) {
            Ok(bytes_read) => {
              if bytes_read == 0 {
                return None;
              }
              Some(future::ok::<(Vec<u8>, u64), _>((
                buf[..bytes_read].to_vec(),
                pos + bytes_read as u64,
              )))
            }
            Err(e) => Some(future::err(e.into())),
          }
        })),
      }))
    }))
  }

  fn set(
    &self,
    key: String,
    data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
    opts: CacheSetOptions,
  ) -> EmptyCacheFuture {
    debug!("sqlite cache set with key: {} and ttl: {:?}", key, opts.ttl);

    let pool = self.pool.clone();

    Box::new(
      data_stream
        .concat2()
        .map_err(|_e| {
          error!("sqlite cache set error concatenating stream");
          CacheError::Unknown
        })
        .and_then(move |b| {
          let conn = pool.get().unwrap(); // TODO: no unwrap

          if let Some(ttl) = opts.ttl {
            let mut stmt = conn
              .prepare(
                "INSERT INTO cache(key, value, meta, expires_at)
      VALUES (?, ?, ?, datetime('now', ?))
      ON CONFLICT (key) DO
        UPDATE SET value=excluded.value,meta=excluded.meta,expires_at=excluded.expires_at
    ",
              )
              .unwrap();

            stmt
              .insert(&[
                &key as &ToSql,
                &b as &ToSql,
                &opts.meta as &ToSql,
                &format!("+{} seconds", ttl) as &ToSql,
              ])
              .unwrap()
          } else {
            let mut stmt = conn
              .prepare(
                "INSERT INTO cache(key, value, meta, expires_at)
      VALUES (?, ?, ?, NULL)
      ON CONFLICT (key) DO
        UPDATE SET value=excluded.value,meta=excluded.meta,expires_at=excluded.expires_at
    ",
              )
              .unwrap();

            stmt
              .insert(&[&key as &ToSql, &b as &ToSql, &opts.meta as &ToSql])
              .unwrap()
          };
          Ok(())
        }),
    )
  }

  fn del(&self, key: String) -> EmptyCacheFuture {
    debug!("sqlite cache del key: {}", key);

    let pool = self.pool.clone();
    Box::new(future::lazy(move || -> Result<(), CacheError> {
      let conn = pool.get().unwrap(); // TODO: no unwrap

      let mut stmt = match conn.prepare("DELETE FROM cache WHERE key = ?") {
        Ok(s) => s,
        Err(e) => return Err(e.into()),
      };
      let ret = match stmt.execute(&[&key]) {
        Ok(r) => r,
        Err(e) => return Err(e.into()),
      };
      debug!("sqlite cache del for key: {} returned: {}", key, ret);
      Ok(())
    }))
  }

  fn expire(&self, key: String, ttl: u32) -> EmptyCacheFuture {
    debug!("sqlite cache expire key: {} w/ ttl: {}", key, ttl);

    let pool = self.pool.clone();
    Box::new(future::lazy(move || -> CacheResult<()> {
      let conn = pool.get().unwrap(); // TODO: no unwrap

      let mut stmt = match conn.prepare(
        "UPDATE cache
      SET expires_at = datetime('now', ?)
      WHERE key = ?",
      ) {
        Ok(s) => s,
        Err(e) => return Err(e.into()),
      };
      let ret = match stmt.execute(&[&format!("+{} seconds", ttl), &key]) {
        Ok(r) => r,
        Err(e) => return Err(e.into()),
      };
      debug!("sqlite cache expire for key: {} returned: {}", key, ret);
      Ok(())
    }))
  }

  fn ttl(&self, _key: String) -> Box<Future<Item = i32, Error = CacheError> + Send> {
    unimplemented!()
  }

  fn purge_tag(&self, _tag: String) -> EmptyCacheFuture {
    unimplemented!()
  }

  fn set_tags(&self, _key: String, _tags: Vec<String>) -> EmptyCacheFuture {
    unimplemented!()
  }

  fn notify(
    &self,
    _op: CacheOperation,
    _value: String,
  ) -> Box<Future<Item = (), Error = CacheStoreNotifierError> + Send> {
    Box::new(future::err(CacheStoreNotifierError::Unavailable))
  }

  fn set_meta(&self, key: String, meta: String) -> EmptyCacheFuture {
    debug!("sqlite cache set_meta key: {}", key);

    let pool = self.pool.clone();
    Box::new(future::lazy(move || -> CacheResult<()> {
      let conn = pool.get().unwrap(); // TODO: no unwrap

      let mut stmt = match conn.prepare(
        "UPDATE cache
      SET meta = ?
      WHERE key = ?",
      ) {
        Ok(s) => s,
        Err(e) => return Err(e.into()),
      };
      let ret = match stmt.execute(&[&meta, &key]) {
        Ok(r) => r,
        Err(e) => return Err(e.into()),
      };
      debug!("sqlite cache set_meta for key: {} returned: {}", key, ret);
      Ok(())
    }))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  extern crate chrono;
  use self::chrono::{DateTime, Utc};

  use std::thread::sleep;
  use std::time::Duration;

  extern crate rand;
  use self::rand::{thread_rng, RngCore};

  fn setup() -> SqliteCacheStore {
    SqliteCacheStore::new("testcache.db".to_string())
  }

  fn set_value(store: &SqliteCacheStore, key: &str, value: &[u8], opts: CacheSetOptions) {
    store
      .set(
        key.to_string(),
        Box::new(stream::once::<Vec<u8>, ()>(Ok(value.to_vec()))),
        opts,
      )
      .wait()
      .unwrap();
  }

  #[test]
  fn test_sqlite_cache_set() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: None,
        meta: None,
        tags: None,
      },
    );

    let pool = &store.pool.clone();
    let conn = pool.get().unwrap(); // TODO: no unwrap

    let mut stmt = conn
      .prepare("SELECT key,value,expires_at FROM cache WHERE key = ? LIMIT 1;")
      .unwrap();

    let mut rows = stmt.query(&[key]).unwrap();
    let row = rows.next().unwrap().unwrap();

    let gotkey: String = row.get(0);
    let gotv: Vec<u8> = row.get(1);
    let gotex: rusqlite::types::Value = row.get(2);

    assert_eq!(gotkey, key);
    assert_eq!(gotv, v.to_vec());
    assert_eq!(gotex, rusqlite::types::Value::Null);
  }

  #[test]
  fn test_sqlite_cache_set_ttl() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:ttl";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: Some(10),
        meta: None,
        tags: None,
      },
    );

    let pool = &store.pool.clone();
    let conn = pool.get().unwrap(); // TODO: no unwrap

    let mut stmt = conn
      .prepare("SELECT key,value,expires_at FROM cache WHERE key = ? AND expires_at > datetime('now') LIMIT 1;")
      .unwrap();

    let mut rows = stmt.query(&[key]).unwrap();
    let row = rows.next().unwrap().unwrap();

    let gotkey: String = row.get(0);
    let gotv: Vec<u8> = row.get(1);
    let gotex: DateTime<Utc> = row.get(2);

    assert_eq!(gotkey, key);
    assert_eq!(gotv, v.to_vec());
    assert!(gotex > Utc::now() && gotex < Utc::now() + chrono::FixedOffset::east(10));
  }

  #[test]
  fn test_sqlite_cache_update_ttl() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:ttl";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: None,
        meta: None,
        tags: None,
      },
    );
    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: Some(10),
        meta: None,
        tags: None,
      },
    );

    let pool = &store.pool.clone();
    let conn = pool.get().unwrap(); // TODO: no unwrap

    let mut stmt = conn
      .prepare("SELECT key,value,expires_at FROM cache WHERE key = ? AND expires_at > datetime('now') LIMIT 1;")
      .unwrap();

    let mut rows = stmt.query(&[key]).unwrap();
    let row = rows.next().unwrap().unwrap();

    let gotkey: String = row.get(0);
    let gotv: Vec<u8> = row.get(1);
    let gotex: DateTime<Utc> = row.get(2);

    assert_eq!(gotkey, key);
    assert_eq!(gotv, v.to_vec());
    assert!(gotex > Utc::now() && gotex < Utc::now() + chrono::FixedOffset::east(10));
  }

  #[test]
  fn test_sqlite_cache_get() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:get";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: None,
        meta: None,
        tags: None,
      },
    );

    let got = store
      .get(key.to_string())
      .wait()
      .unwrap()
      .unwrap()
      .stream
      .concat2()
      .wait()
      .unwrap();

    assert_eq!(got, v.to_vec());
  }

  #[test]
  fn test_sqlite_cache_get_ttl() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:get:ttl";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: Some(10),
        meta: None,
        tags: None,
      },
    );

    let got = store
      .get(key.to_string())
      .wait()
      .unwrap()
      .unwrap()
      .stream
      .concat2()
      .wait()
      .unwrap();

    assert_eq!(got, v.to_vec());
  }

  #[test]
  fn test_sqlite_cache_get_expired() {
    let store = setup();
    let v = [0u8; 1];
    let key = "test:get:expired";
    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: Some(1),
        meta: None,
        tags: None,
      },
    );

    sleep(Duration::from_secs(2)); // inefficient, but these are just tests

    let entry = store.get(key.to_string()).wait().unwrap();

    assert!(entry.is_none());
  }

  #[test]
  fn test_sqlite_cache_del() {
    let store = setup();
    let v = [0u8; 1];
    let key = "test:del";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: None,
        meta: None,
        tags: None,
      },
    );

    store.del(key.to_string()).wait().unwrap();

    let entry = store.get(key.to_string()).wait().unwrap();

    assert!(entry.is_none());
  }

  #[test]
  fn test_sqlite_cache_expire() {
    let store = setup();
    let v = [0u8; 1];
    let key = "test:expire";

    set_value(
      &store,
      key,
      &v,
      CacheSetOptions {
        ttl: None,
        meta: None,
        tags: None,
      },
    );

    let pool = &store.pool.clone();
    let conn = pool.get().unwrap(); // TODO: no unwrap

    let mut stmt = conn
      .prepare("SELECT expires_at FROM cache WHERE key = ? LIMIT 1;")
      .unwrap();

    {
      let mut rows = stmt.query(&[key]).unwrap();
      let row = rows.next().unwrap().unwrap();

      let gotex: rusqlite::types::Value = row.get(0);

      assert_eq!(gotex, rusqlite::types::Value::Null);
    }

    store.expire(key.to_string(), 10).wait().unwrap();

    let mut rows = stmt.query(&[key]).unwrap();
    let row = rows.next().unwrap().unwrap();

    let gotex: DateTime<Utc> = row.get(0);

    assert!(gotex > Utc::now() && gotex < Utc::now() + chrono::FixedOffset::east(10));
  }
}
