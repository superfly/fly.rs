use data::*;
use std::sync::Arc;

extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;
use self::r2d2_postgres::PostgresConnectionManager;

use self::postgres::params::{Builder, ConnectParams, IntoConnectParams};
use self::postgres::types::ToSql;
use self::postgres::{Connection, TlsMode};

extern crate serde_json;

use futures::{future, Future};

pub struct PostgresDataStore {
  pool: Arc<r2d2::Pool<PostgresConnectionManager>>,
}

impl PostgresDataStore {
  pub fn new(url: String, maybe_dbname: Option<String>) -> Self {
    let params: ConnectParams = url.into_connect_params().unwrap();
    let mut builder = Builder::new();
    builder.port(params.port());
    if let Some(user) = params.user() {
      builder.user(user.name(), user.password());
    }

    // let dbname: String = if let Some(dbname) = &maybe_dbname {
    //   dbname.clone()
    // } else if let Some(dbname) = params.database() {
    //   dbname.to_string()
    // } else {
    //   panic!("postgres database name required");
    // };

    if let Some(dbname) = params.database() {
      builder.database(dbname);
    }

    let params = builder.build(params.host().clone());

    println!("params: {:?}", params);

    let pool = if let Some(dbname) = &maybe_dbname {
      println!("cloned params: {:?}", params.clone());
      let conn = Connection::connect(params.clone(), TlsMode::None).unwrap();
      match conn.execute(&format!("CREATE DATABASE \"{}\"", dbname), NO_PARAMS) {
        Ok(_) => debug!("database created with success"),
        Err(e) => warn!(
          "could not create database, either it already existed or we didn't have permission! {}",
          e
        ),
      };

      builder.database(&dbname);
      builder.port(params.port());
      if let Some(user) = params.user() {
        builder.user(user.name(), user.password());
      }

      let pool_params = builder.build(params.host().clone());
      println!("pool params: {:?}", pool_params);
      let manager =
        PostgresConnectionManager::new(pool_params, r2d2_postgres::TlsMode::None).unwrap();
      r2d2::Pool::builder().build(manager).unwrap()
    } else {
      let manager = PostgresConnectionManager::new(params, r2d2_postgres::TlsMode::None).unwrap();
      r2d2::Pool::builder().build(manager).unwrap()
    };
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
        format!("SELECT obj::text FROM {} WHERE key = $1", coll).as_str(),
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
        format!("DELETE FROM {} WHERE key = $1", coll).as_str(),
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
        &format!(
          "INSERT INTO {} (key, obj) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET obj = excluded.obj",
          coll
        ),
        &[&key, &serde_json::from_str::<serde_json::Value>(&data).unwrap()],
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
      "CREATE TABLE IF NOT EXISTS {} (key TEXT PRIMARY KEY NOT NULL, obj JSONB NOT NULL)",
      name
    ).as_str(),
    NO_PARAMS,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn setup(dbname: Option<String>) -> PostgresDataStore {
    PostgresDataStore::new(
      "postgres://jerome@localhost:5432/postgres".to_string(),
      dbname,
    )
  }

  fn teardown(dbname: &str) {
    let conn =
      Connection::connect("postgres://jerome@localhost:5432/postgres", TlsMode::None).unwrap();

    conn
      .execute(&format!("DROP DATABASE {}", dbname), NO_PARAMS)
      .unwrap();
  }

  fn set_value(
    store: &PostgresDataStore,
    coll: &str,
    key: &str,
    value: &str,
    maybe_el: Option<&mut tokio::runtime::Runtime>,
  ) {
    let setfut = store.put(coll.to_string(), key.to_string(), value.to_string());

    match maybe_el {
      Some(el) => el.block_on(setfut).unwrap(),
      None => tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(setfut)
        .unwrap(),
    };
  }

  #[test]
  fn test_some_dbname() {
    let dbname = "testflydata";
    let res: String = {
      let store = setup(Some(dbname.to_string()));
      let pool = Arc::clone(&store.pool);
      let conn = pool.get().unwrap();

      conn
        .query(
          &format!(
            "SELECT datname FROM pg_database WHERE datname = '{}' LIMIT 1",
            &dbname
          ),
          NO_PARAMS,
        ).unwrap()
        .get(0)
        .get(0)
    };

    teardown(&dbname);

    assert_eq!(&res, dbname);
  }

  #[test]
  fn test_put_get() {
    let dbname = "testflyputget";
    let coll = "coll1";
    let key = "test:key";
    let value = r#"{"foo":"bar"}"#;
    let got = {
      let store = setup(Some(dbname.to_string()));
      let mut el = tokio::runtime::Runtime::new().unwrap();
      set_value(&store, coll, key, value, Some(&mut el));

      el.block_on(store.get(coll.to_string(), key.to_string()))
        .unwrap()
        .unwrap()
    };

    teardown(&dbname);

    assert_eq!(
      serde_json::from_str::<serde_json::Value>(&got).unwrap(),
      serde_json::from_str::<serde_json::Value>(value).unwrap()
    );
  }

}
