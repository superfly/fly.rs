use data::*;
use std::sync::Arc;

extern crate r2d2;
extern crate r2d2_sqlite;
extern crate rusqlite;
use self::r2d2_sqlite::SqliteConnectionManager;
use self::rusqlite::NO_PARAMS;

use futures::{future, Future};

pub struct SqliteDataStore {
  pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
}

impl SqliteDataStore {
  pub fn new(filename: String) -> Self {
    let manager = SqliteConnectionManager::file(filename);
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    SqliteDataStore {
      pool: Arc::new(pool),
    }
  }
}

impl From<rusqlite::Error> for DataError {
  #[inline]
  fn from(err: rusqlite::Error) -> DataError {
    DataError::Failure(format!("{}", err))
  }
}

impl DataStore for SqliteDataStore {
  fn get(
    &self,
    coll: String,
    key: String,
  ) -> Box<Future<Item = Option<String>, Error = DataError> + Send> {
    debug!("sqlite data store get coll: {}, key: {}", coll, key);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<Option<String>> {
      let con = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*con, &coll).unwrap();

      match con.query_row::<String, _, _>(
        format!("SELECT obj FROM {} WHERE key == ?", coll).as_str(),
        &[&key],
        |row| row.get(0),
      ) {
        Err(e) => {
          if let rusqlite::Error::QueryReturnedNoRows = e {
            Ok(None)
          } else {
            Err(e.into())
          }
        }
        Ok(s) => Ok(Some(s)),
      }
    }))
  }

  fn del(&self, coll: String, key: String) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("sqlite data store del coll: {}, key: {}", coll, key);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<()> {
      let con = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*con, &coll).unwrap();

      match con.execute(
        format!("DELETE FROM {} WHERE key == ?", coll).as_str(),
        &[&key],
      ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }

  fn put(
    &self,
    coll: String,
    key: String,
    data: String,
  ) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("sqlite data store put coll: {}, key: {}", coll, key);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<()> {
      let con = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*con, &coll).unwrap();
      match con.execute(
        format!("INSERT OR REPLACE INTO {} VALUES (?, ?)", coll).as_str(),
        &[&key, &data],
      ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }

  fn drop_coll(&self, coll: String) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("sqlite data store drop coll: {}", coll);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<()> {
      let con = pool.get().unwrap(); // TODO: no unwrap
      match con.execute(
        format!("DROP TABLE IF EXISTS {}", coll).as_str(),
        rusqlite::NO_PARAMS,
      ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }
}

fn ensure_coll(conn: &rusqlite::Connection, name: &str) -> rusqlite::Result<usize> {
  conn.execute(
    format!(
      "CREATE TABLE IF NOT EXISTS {} (key TEXT PRIMARY KEY NOT NULL, obj JSON NOT NULL)",
      name
    ).as_str(),
    NO_PARAMS,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn setup() -> SqliteDataStore {
    SqliteDataStore::new("testdata.db".to_string())
  }

  fn set_value(
    store: &SqliteDataStore,
    coll: &str,
    key: &str,
    value: &str,
    maybe_el: Option<&mut tokio::runtime::Runtime>,
  ) {
    let setfut = store.put(coll.to_string(), key.to_string(), value.to_string());

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
  fn test_put_get() {
    let store = setup();
    let mut el = tokio::runtime::Runtime::new().unwrap();
    let coll = "coll1";
    let key = "test:key";
    let value = r#"{"foo": "bar"}"#;
    set_value(&store, coll, key, value, Some(&mut el));

    let got = el
      .block_on(store.get(coll.to_string(), key.to_string()))
      .unwrap()
      .unwrap();

    assert_eq!(got, value.to_string());
  }

  #[test]
  fn test_del() {
    let store = setup();
    let mut el = tokio::runtime::Runtime::new().unwrap();
    let coll = "coll1";
    let key = "test:key";
    let value = "{}";
    set_value(&store, coll, key, value, Some(&mut el));

    let got_res = el
      .block_on(store.get(coll.to_string(), key.to_string()))
      .unwrap()
      .unwrap();
    assert_eq!(got_res, value.to_string());

    el.block_on(store.del(coll.to_string(), key.to_string()))
      .unwrap();

    let got = el.block_on(store.get(coll.to_string(), key.to_string()));

    assert!(got.is_err());
    assert_eq!(got.err().unwrap(), DataError::Failure("asda".to_string()));
  }
}
