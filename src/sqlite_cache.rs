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

use std::io;

#[derive(Debug)]
pub enum CacheError {
  Unknown,
  RusqliteErr(rusqlite::Error),
  IoErr(io::Error),
}

impl From<io::Error> for CacheError {
  #[inline]
  fn from(err: io::Error) -> CacheError {
    CacheError::IoErr(err)
  }
}

impl From<rusqlite::Error> for CacheError {
  #[inline]
  fn from(err: rusqlite::Error) -> CacheError {
    CacheError::RusqliteErr(err)
  }
}

use config::CONFIG;

lazy_static! {
  static ref SQLITE_CACHE_POOL: Arc<r2d2::Pool<SqliteConnectionManager>> = {
    let manager = SqliteConnectionManager::file(match CONFIG.read().unwrap().get::<String>("cache.sqlite.filename"){
      Ok(s) => s,
      Err(_e) => "cache.db".to_string(),
    });
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    let con = pool.get().unwrap(); // TODO: no unwrap
    con.execute("CREATE TABLE IF NOT EXISTS cache (
      key TEXT PRIMARY KEY NOT NULL,
      value BLOB NOT NULL,
      expires_at DATETIME
    );
    CREATE UNIQUE INDEX IF NOT EXISTS ON cache (key);
    CREATE INDEX IF NOT EXISTS ON cache (key, expires_at);",
    NO_PARAMS,
    ).unwrap();
    Arc::new(pool)
  };
}

pub fn set(
  key: String,
  maybe_ttl: Option<u32>,
  data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
) -> Box<Future<Item = (), Error = CacheError> + Send> {
  debug!(
    "sqlite cache set with key: {} and ttl: {:?}",
    key, maybe_ttl
  );

  Box::new(
    data_stream
      .concat2()
      .map_err(|_e| {
        error!("sqlite cache set error concatenating stream");
        CacheError::Unknown
      }).and_then(move |b| {
        let pool = Arc::clone(&SQLITE_CACHE_POOL);
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
              "INSERT INTO cache(key, value)
      VALUES (?, ?)
      ON CONFLICT (key) DO
        UPDATE SET value=excluded.value
    ",
            ).unwrap();

          stmt.insert(&[&key as &ToSql, &b as &ToSql]).unwrap()
        };
        Ok(())
      }),
  )
}

pub fn get(
  key: String,
) -> Result<Option<Box<Stream<Item = Vec<u8>, Error = CacheError> + Send>>, CacheError> {
  debug!("sqlite cache get with key: {}", key);
  let pool = Arc::clone(&SQLITE_CACHE_POOL);
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

  debug!("got rows");

  let rowid: i64 = match rows.next() {
    Some(res) => res?.get(0),
    None => {
      debug!("row not found");
      return Ok(None);
    }
  };

  debug!("got a rowid: {}", rowid);

  Ok(Some(Box::new(stream::unfold(0, move |pos| {
    debug!("sqlite cache get in stream future, pos: {}", pos);

    // End early given some rules!
    // not a multiple of size, means we're done.
    if pos > 0 && pos % size > 0 {
      debug!("sqlite cache get returning early");
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

#[cfg(test)]
mod tests {
  // Note this useful idiom: importing names from outer (for mod tests) scope.
  use super::*;
  use config::CONFIG;
  use futures::sync::mpsc;

  extern crate chrono;
  use self::chrono::{DateTime, Utc};

  use std::thread::sleep;
  use std::time::Duration;

  extern crate rand;
  use self::rand::{thread_rng, Rng, RngCore};

  fn setup() {
    {
      CONFIG
        .write()
        .unwrap()
        .set("cache.sqlite.filename", "testcache.db")
        .unwrap()
    };
  }

  fn set_value(
    key: &str,
    value: &[u8],
    ttl: Option<u32>,
    maybe_el: Option<&mut tokio::runtime::Runtime>,
  ) {
    let setfut = {
      let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
      let setfut = set(key.to_string(), ttl, Box::new(recver));
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
    setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test";

    set_value(key, &v, None, None);

    let pool = Arc::clone(&SQLITE_CACHE_POOL);
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
    setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:ttl";

    set_value(key, &v, Some(10), None);

    let pool = Arc::clone(&SQLITE_CACHE_POOL);
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
    setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:ttl";

    set_value(key, &v, None, None);
    set_value(key, &v, Some(10), None);

    let pool = Arc::clone(&SQLITE_CACHE_POOL);
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
    setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:get";

    let mut el = tokio::runtime::Runtime::new().unwrap();
    set_value(key, &v, None, Some(&mut el));

    let got = el
      .block_on(get(key.to_string()).unwrap().unwrap().concat2())
      .unwrap();

    assert_eq!(got, v.to_vec());
  }

  #[test]
  fn test_get_ttl() {
    setup();
    let mut v = [0u8; 1000000];
    thread_rng().fill_bytes(&mut v);
    let key = "test:get:ttl";

    let mut el = tokio::runtime::Runtime::new().unwrap();
    set_value(key, &v, Some(10), Some(&mut el));

    let got = el
      .block_on(get(key.to_string()).unwrap().unwrap().concat2())
      .unwrap();

    assert_eq!(got, v.to_vec());
  }

  #[test]
  fn test_get_expired() {
    setup();
    let v = [0u8; 1];
    let key = "test:get:expired";
    set_value(key, &v, Some(1), None);

    sleep(Duration::from_secs(2)); // inefficient, but these are just tests

    let stream = get(key.to_string()).unwrap();

    assert!(stream.is_none());
  }
}
