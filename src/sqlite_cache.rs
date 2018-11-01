use std::sync::Arc;

extern crate r2d2;
extern crate r2d2_sqlite;
extern crate rusqlite;
use self::r2d2_sqlite::SqliteConnectionManager;

use self::rusqlite::types::ToSql;
use self::rusqlite::NO_PARAMS;

use futures::{future, stream, Future, Stream};
use std::ops::Deref;

use std::io::{Read, Seek, SeekFrom};

use cache::*;

impl From<rusqlite::Error> for CacheError {
  #[inline]
  fn from(err: rusqlite::Error) -> CacheError {
    CacheError::Failure(format!("{}", err))
  }
}

pub struct SqliteCacheStore {
  pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
}

impl SqliteCacheStore {
  pub fn new(filename: String) -> Self {
    let manager = SqliteConnectionManager::file(filename);
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    let con = pool.get().unwrap(); // TODO: no unwrap
    con
      .execute(
        "CREATE TABLE IF NOT EXISTS cache (
      key TEXT PRIMARY KEY NOT NULL,
      value BLOB NOT NULL,
      expires_at DATETIME
    );
    CREATE UNIQUE INDEX IF NOT EXISTS ON cache (key);
    CREATE INDEX IF NOT EXISTS ON cache (key, expires_at);",
        NO_PARAMS,
      ).unwrap();

    SqliteCacheStore {
      pool: Arc::new(pool),
    }
  }
}

impl CacheStore for SqliteCacheStore {
  fn get(
    &self,
    key: String,
  ) -> CacheResult<Option<Box<Stream<Item = Vec<u8>, Error = CacheError> + Send>>> {
    debug!("sqlite cache get with key: {}", key);
    let pool = Arc::clone(&self.pool);
    let conn = pool.get().unwrap(); // TODO: no unwrap
    let size = 256 * 1024;

    let mut stmt = conn
      .prepare(
        "SELECT rowid FROM cache
      WHERE key = ? AND
        (
          expires_at IS NULL OR
          expires_at >= datetime('now')
        ) LIMIT 1",
      ).unwrap();

    let mut rows = stmt.query(&[&key])?;

    let rowid: i64 = match rows.next() {
      Some(res) => res?.get(0),
      None => {
        debug!("row not found");
        return Ok(None);
      }
    };

    Ok(Some(Box::new(stream::unfold(0, move |pos| {
      debug!("sqlite cache get in stream future, pos: {}", pos);

      // End early given some rules!
      // not a multiple of size, means we're done.
      if pos > 0 && pos % size > 0 {
        return None;
      }

      let conn = pool.get().unwrap();

      let mut blob =
        match conn
          .deref()
          .blob_open(rusqlite::DatabaseName::Main, "cache", "value", rowid, true)
        {
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
    }))))
  }

  fn set(
    &self,
    key: String,
    data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
    maybe_ttl: Option<u32>,
  ) -> Box<Future<Item = (), Error = CacheError> + Send> {
    debug!(
      "sqlite cache set with key: {} and ttl: {:?}",
      key, maybe_ttl
    );

    let pool = Arc::clone(&self.pool);

    Box::new(
      data_stream
        .concat2()
        .map_err(|_e| {
          error!("sqlite cache set error concatenating stream");
          CacheError::Unknown
        }).and_then(move |b| {
          let conn = pool.get().unwrap(); // TODO: no unwrap

          if let Some(ttl) = maybe_ttl {
            let mut stmt = conn
              .prepare(
                "INSERT INTO cache(key, value, expires_at)
      VALUES (?, ?, datetime('now', ?))
      ON CONFLICT (key) DO
        UPDATE SET value=excluded.value,expires_at=excluded.expires_at
    ",
              ).unwrap();

            stmt
              .insert(&[
                &key as &ToSql,
                &b as &ToSql,
                &format!("+{} seconds", ttl) as &ToSql,
              ]).unwrap()
          } else {
            let mut stmt = conn
              .prepare(
                "INSERT INTO cache(key, value, expires_at)
      VALUES (?, ?, NULL)
      ON CONFLICT (key) DO
        UPDATE SET value=excluded.value,expires_at=excluded.expires_at
    ",
              ).unwrap();

            stmt.insert(&[&key as &ToSql, &b as &ToSql]).unwrap()
          };
          Ok(())
        }),
    )
  }

  fn del(&self, key: String) -> Box<Future<Item = (), Error = CacheError> + Send> {
    debug!("sqlite cache del key: {}", key);

    let pool = Arc::clone(&self.pool);
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

  fn expire(&self, key: String, ttl: u32) -> Box<Future<Item = (), Error = CacheError> + Send> {
    debug!("sqlite cache expire key: {} w/ ttl: {}", key, ttl);

    let pool = Arc::clone(&self.pool);
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
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::sync::mpsc;

  extern crate chrono;
  use self::chrono::{DateTime, Utc};

  use std::thread::sleep;
  use std::time::Duration;

  extern crate rand;
  use self::rand::{thread_rng, RngCore};

  fn setup() -> SqliteCacheStore {
    SqliteCacheStore::new("testcache.db".to_string())
  }

  fn set_value(
    store: &SqliteCacheStore,
    key: &str,
    value: &[u8],
    ttl: Option<u32>,
    maybe_el: Option<&mut tokio::runtime::Runtime>,
  ) {
    let setfut = {
      let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
      let setfut = store.set(key.to_string(), Box::new(recver), ttl);
      sender.unbounded_send(value.to_vec()).unwrap();
      setfut
    };

    let res = match maybe_el {
      Some(el) => el.block_on(setfut).unwrap(),
      None => tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(setfut)
        .unwrap(),
    };
    assert_eq!(res, ());
  }

  #[test]
  fn test_set() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test";

    set_value(&store, key, &v, None, None);

    let pool = Arc::clone(&store.pool);
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
  fn test_set_ttl() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:ttl";

    set_value(&store, key, &v, Some(10), None);

    let pool = Arc::clone(&store.pool);
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
  fn test_update_ttl() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:ttl";

    set_value(&store, key, &v, None, None);
    set_value(&store, key, &v, Some(10), None);

    let pool = Arc::clone(&store.pool);
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
  fn test_get() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:get";

    let mut el = tokio::runtime::Runtime::new().unwrap();
    set_value(&store, key, &v, None, Some(&mut el));

    let got = el
      .block_on(store.get(key.to_string()).unwrap().unwrap().concat2())
      .unwrap();

    assert_eq!(got, v.to_vec());
  }

  #[test]
  fn test_get_ttl() {
    let store = setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:get:ttl";

    let mut el = tokio::runtime::Runtime::new().unwrap();
    set_value(&store, key, &v, Some(10), Some(&mut el));

    let got = el
      .block_on(store.get(key.to_string()).unwrap().unwrap().concat2())
      .unwrap();

    assert_eq!(got, v.to_vec());
  }

  #[test]
  fn test_get_expired() {
    let store = setup();
    let v = [0u8; 1];
    let key = "test:get:expired";
    set_value(&store, key, &v, Some(1), None);

    sleep(Duration::from_secs(2)); // inefficient, but these are just tests

    let stream = store.get(key.to_string()).unwrap();

    assert!(stream.is_none());
  }

  #[test]
  fn test_del() {
    let store = setup();
    let v = [0u8; 1];
    let key = "test:del";

    let mut el = tokio::runtime::Runtime::new().unwrap();
    set_value(&store, key, &v, None, Some(&mut el));

    let res = el.block_on(store.del(key.to_string())).unwrap();
    assert_eq!(res, ());

    let stream = store.get(key.to_string()).unwrap();

    assert!(stream.is_none());
  }

  #[test]
  fn test_expire() {
    let store = setup();
    let v = [0u8; 1];
    let key = "test:expire";

    let mut el = tokio::runtime::Runtime::new().unwrap();
    set_value(&store, key, &v, None, Some(&mut el));

    let pool = Arc::clone(&store.pool);
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

    let res = el.block_on(store.expire(key.to_string(), 10)).unwrap();
    assert_eq!(res, ());

    // let mut stmt = conn
    //   .prepare("SELECT expires_at FROM cache WHERE key = ? LIMIT 1;")
    //   .unwrap();

    let mut rows = stmt.query(&[key]).unwrap();
    let row = rows.next().unwrap().unwrap();

    let gotex: DateTime<Utc> = row.get(0);

    assert!(gotex > Utc::now() && gotex < Utc::now() + chrono::FixedOffset::east(10));
  }
}
