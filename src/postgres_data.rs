use data::*;
use std::sync::Arc;

extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;
use self::r2d2_postgres::PostgresConnectionManager;

use self::postgres::types::ToSql;

use futures::{future, Future};

pub struct PostgresDataStore {
  pool: Arc<r2d2::Pool<PostgresConnectionManager>>,
}

impl PostgresDataStore {
  pub fn new(url: String) -> Self {
    let manager = PostgresConnectionManager::new(url, r2d2_postgres::TlsMode::None).unwrap();
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    PostgresDataStore {
      pool: Arc::new(pool),
    }
  }
}

const NO_PARAMS: &'static [&'static ToSql] = &[];

impl From<postgres::Error> for DataError {
  #[inline]
  fn from(err: postgres::Error) -> DataError {
    DataError::Failure(format!("{}", err))
  }
}

impl DataStore for PostgresDataStore {
  fn get(
    &self,
    coll: String,
    key: String,
  ) -> Box<Future<Item = Option<String>, Error = DataError> + Send> {
    debug!("postgres data store get coll: {}, key: {}", coll, key);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<Option<String>> {
      let conn = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*conn, &coll).unwrap();

      match conn.query(
        format!("SELECT obj FROM {} WHERE key == ? LIMIT 1", coll).as_str(),
        &[&key],
      ) {
        Err(e) => return Err(e.into()),
        Ok(rows) => {
          if rows.len() == 0 {
            return Ok(None);
          }
          Ok(Some(rows.get(0).get("obj")))
        }
      }
    }))
  }

  fn del(&self, coll: String, key: String) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("postgres data store del coll: {}, key: {}", coll, key);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<()> {
      let conn = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*conn, &coll).unwrap();

      match conn.execute(
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
    debug!("postgres data store put coll: {}, key: {}", coll, key);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<()> {
      let conn = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*conn, &coll).unwrap();
      match conn.execute(
        format!("INSERT OR REPLACE INTO {} VALUES (?, ?)", coll).as_str(),
        &[&key, &data],
      ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }

  fn drop_coll(&self, coll: String) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("postgres data store drop coll: {}", coll);
    let pool = Arc::clone(&self.pool);
    Box::new(future::lazy(move || -> DataResult<()> {
      let con = pool.get().unwrap(); // TODO: no unwrap
      match con.execute(format!("DROP TABLE IF EXISTS {}", coll).as_str(), NO_PARAMS) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }
}

fn ensure_coll(conn: &postgres::Connection, name: &str) -> postgres::Result<u64> {
  conn.execute(
    format!(
      "CREATE TABLE IF NOT EXISTS {} (key TEXT PRIMARY KEY NOT NULL, obj JSON NOT NULL)",
      name
    ).as_str(),
    NO_PARAMS,
  )
}
