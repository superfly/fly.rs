use crate::data_store::*;

extern crate r2d2;
extern crate r2d2_sqlite;
extern crate rusqlite;
use self::r2d2_sqlite::SqliteConnectionManager;
use self::rusqlite::NO_PARAMS;

use futures::{future, Future};

pub struct SqliteDataStore {
  pool: r2d2::Pool<SqliteConnectionManager>,
}

impl SqliteDataStore {
  pub fn new(filename: String) -> Self {
    SqliteDataStore {
      pool: r2d2::Pool::new(SqliteConnectionManager::file(filename)).unwrap(),
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
    let pool = self.pool.clone();
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
    let pool = self.pool.clone();
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
    let pool = self.pool.clone();
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

  fn incr(
    &self,
    coll: String,
    key: String,
    field: String,
    amount: i32,
  ) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!(
      "sqlite data store incr coll: {}, key: {}, amount: {}",
      coll, key, amount
    );
    let pool = self.pool.clone();
    Box::new(future::lazy(move || -> DataResult<()> {
      let con = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*con, &coll).unwrap();

      let selector = format!("$.{}", field);

      match con.execute(
        format!(
          "UPDATE {} SET obj = json_set(obj, '{}', COALESCE(json_extract(obj, '{}'), '0') + ?) WHERE key == ?",
          coll, selector, selector
        )
        .as_str(),
        &[&amount.to_string(), &key],
      ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }

  fn drop_coll(&self, coll: String) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("sqlite data store drop coll: {}", coll);
    let pool = self.pool.clone();
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
    )
    .as_str(),
    NO_PARAMS,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn setup() -> SqliteDataStore {
    SqliteDataStore::new("testdata.db".to_string())
  }

  fn set_value(store: &SqliteDataStore, coll: &str, key: &str, value: &str) {
    store
      .put(coll.to_string(), key.to_string(), value.to_string())
      .wait()
      .unwrap();
  }

  #[test]
  fn test_sqlite_data_put_get() {
    let store = setup();
    let coll = "coll1";
    let key = "test:key";
    let value = r#"{"foo": "bar"}"#;
    set_value(&store, coll, key, value);

    let got = store
      .get(coll.to_string(), key.to_string())
      .wait()
      .unwrap()
      .unwrap();

    assert_eq!(got, value.to_string());
  }

  #[test]
  fn test_sqlite_data_incr() {
    let store = setup();
    let coll = "collincr";
    let key = "test:key";
    let value = r#"{"counter": 0, "foo": "bar"}"#;
    set_value(&store, coll, key, value);

    store
      .incr(coll.to_string(), key.to_string(), "counter".to_string(), 1)
      .wait()
      .unwrap();
    let got = store
      .get(coll.to_string(), key.to_string())
      .wait()
      .unwrap()
      .unwrap();

    assert_eq!(got, r#"{"counter":1,"foo":"bar"}"#);

    store
      .incr(coll.to_string(), key.to_string(), "counter".to_string(), 15)
      .wait()
      .unwrap();
    let got = store
      .get(coll.to_string(), key.to_string())
      .wait()
      .unwrap()
      .unwrap();

    assert_eq!(got, r#"{"counter":16,"foo":"bar"}"#);
  }

  #[test]
  fn test_sqlite_data_del() {
    let store = setup();
    let mut el = tokio::runtime::Runtime::new().unwrap();
    let coll = "coll1";
    let key = "test:key";
    let value = "{}";
    set_value(&store, coll, key, value);

    let got_res = el
      .block_on(store.get(coll.to_string(), key.to_string()))
      .unwrap()
      .unwrap();
    assert_eq!(got_res, value.to_string());

    el.block_on(store.del(coll.to_string(), key.to_string()))
      .unwrap();

    let got = el
      .block_on(store.get(coll.to_string(), key.to_string()))
      .unwrap();

    assert!(got.is_none());
  }
}
