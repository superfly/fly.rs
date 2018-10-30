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

lazy_static! {
  static ref SQLITE_CACHE_POOL: Arc<r2d2::Pool<SqliteConnectionManager>> = {
    let manager = SqliteConnectionManager::file("cache.db");
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
      VALUES (?, ?, datetime('now', '+? seconds'))
      ON CONFLICT (key) DO
        UPDATE SET value=excluded.value,expires_at=excluded.expires_at
    ",
            ).unwrap();

          stmt
            .insert(&[&key as &ToSql, &b as &ToSql, &format!("{}", ttl) as &ToSql])
            .unwrap()
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

        // let mut stmt = conn
        //   .prepare("SELECT rowid FROM cache WHERE key = ? LIMIT 1")
        //   .unwrap();

        // let mut rows = match stmt.query(&[&key]) {
        //   Ok(r) => r,
        //   Err(e) => return Err(e.into()),
        // };
        // let rowid: i64 = match rows.next() {
        //   Some(res) => match res {
        //     Ok(r) => r.get(0),
        //     Err(e) => return Err(e.into()),
        //   },
        //   None => return Err(CacheError::Unknown),
        // };
        Ok(())
      }),
  )

  // Ok(Box::new(
  //   data_stream
  //     .map_err(|e| {
  //       error!("error cache set stream!");
  //       CacheError::Unknown
  //     }).map(move |b| -> Result<(), CacheError> {
  //       let start = offset.fetch_add(b.len(), Ordering::SeqCst) as u64;
  //       debug!("sqlite cache set len: {} at offset: {}", b.len(), start);

  //       let conn = pool.get().unwrap();

  //       let mut blob =
  //         match conn
  //           .deref()
  //           .blob_open(rusqlite::DatabaseName::Main, "cache", "value", rowid, false)
  //         {
  //           Ok(b) => b,
  //           Err(e) => {
  //             return Err(e.into());
  //           }
  //         };

  //       // if let Err(e) = blob.seek(SeekFrom::Start(start)) {
  //       //   return Err(e.into());
  //       // }

  //       match blob.write(b.as_slice()) {
  //         Ok(n) => debug!("sqlite cache set set, wrote {}", n),
  //         Err(e) => return Err(e.into()),
  //       };

  //       Ok(())
  //     }).and_then(|res| res), // uh, ok. that's required!
  // ))
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
          expires_at <= datetime('now')
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
    // println!("unfolding... pos: {}, modulo: {}", pos, pos % size);

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
