use crate::data_store::*;

use r2d2_postgres::{PostgresConnectionManager, TlsMode};

use openssl::ssl::{SslConnector, SslFiletype};
use postgres::params::{Builder, ConnectParams, IntoConnectParams};
use postgres::types::ToSql;
use postgres::Connection;
use postgres_openssl::openssl::ssl::SslMethod;

use crate::settings::PostgresStoreConfig;

use serde_json;

use std::collections::HashMap;

use std::sync::Mutex;

use futures::{future, Future};

lazy_static! {
  static ref PG_POOLS: Mutex<HashMap<String, r2d2::Pool<PostgresConnectionManager>>> =
    Mutex::new(HashMap::new());
}

pub struct PostgresDataStore {
  url: String,
  db: Option<String>,
  tls: Option<SslConnector>,
}

impl PostgresDataStore {
  pub fn new(conf: &PostgresStoreConfig) -> Self {
    let url = conf.url.clone();

    let tls = if conf.tls_client_crt.is_some() {
      let mut connbuilder = SslConnector::builder(SslMethod::tls()).unwrap();
      if let Some(ref ca) = conf.tls_ca_crt {
        connbuilder.set_ca_file(ca).unwrap();
      }
      connbuilder
        .set_certificate_file(conf.tls_client_crt.as_ref().unwrap(), SslFiletype::PEM)
        .unwrap();
      connbuilder
        .set_private_key_file(conf.tls_client_key.as_ref().unwrap(), SslFiletype::PEM)
        .unwrap();
      Some(connbuilder.build())
    } else {
      None
    };

    PostgresDataStore {
      url,
      db: conf.database.as_ref().cloned(),
      tls,
    }
  }

  fn get_pool(&self) -> r2d2::Pool<PostgresConnectionManager> {
    let key = format!(
      "{}:{}",
      self.url,
      self.db.as_ref().unwrap_or(&"".to_string())
    );

    PG_POOLS
      .lock()
      .unwrap()
      .entry(key)
      .or_insert_with(move || {
        if let Err(e) = self.ensure_db() {
          if let Some(edb) = e.as_db().cloned() {
            // db error
            if !edb.message.contains("already exists") {
              panic!(edb);
            }
          } else {
            panic!(e);
          }
        }; // TODO: bubble that error up

        let params: ConnectParams = self.url.as_str().into_connect_params().unwrap();
        let mut builder = Builder::new();
        builder.port(params.port());
        if let Some(user) = params.user() {
          builder.user(user.name(), user.password());
        }

        if let Some(ref dbname) = self.db {
          builder.database(dbname);
        } else if let Some(dbname) = params.database() {
          builder.database(dbname);
        }

        let params = builder.build(params.host().clone());
        let manager = PostgresConnectionManager::new(
          params,
          match self.tls {
            Some(ref tls) => {
              TlsMode::Require(Box::new(postgres_openssl::OpenSsl::from(tls.clone())))
            }
            None => TlsMode::None,
          },
        )
        .unwrap();
        r2d2::Pool::builder().build(manager).unwrap()
      })
      .clone()
  }

  fn ensure_db(&self) -> postgres::Result<u64> {
    if let Some(ref dbname) = self.db {
      let params: ConnectParams = self.url.as_str().into_connect_params().unwrap();
      let mut builder = Builder::new();
      builder.port(params.port());
      if let Some(user) = params.user() {
        builder.user(user.name(), user.password());
      }

      if let Some(dbname) = params.database() {
        builder.database(dbname);
      }

      let params = builder.build(params.host().clone());

      let tls_connector = match self.tls {
        Some(ref tls) => Some(postgres_openssl::OpenSsl::from(tls.clone())),
        None => None,
      };
      let conn = Connection::connect(
        params.clone(),
        match tls_connector {
          Some(ref connector) => postgres::TlsMode::Require(connector),
          None => postgres::TlsMode::None,
        },
      )
      .unwrap();
      conn.execute(&format!("CREATE DATABASE \"{}\"", dbname), NO_PARAMS)
    } else {
      Ok(0)
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
    let pool = self.get_pool();
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
    let pool = self.get_pool();
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
    let pool = self.get_pool();
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

  fn incr(
    &self,
    coll: String,
    key: String,
    field: String,
    amount: i32,
  ) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!(
      "postgres data store incr coll: {}, key: {}, field: {}, amount: {}",
      coll, key, field, amount
    );
    let pool = self.get_pool();
    Box::new(future::lazy(move || -> DataResult<()> {
      let con = pool.get().unwrap(); // TODO: no unwrap

      ensure_coll(&*con, &coll).unwrap();

      match con.execute(
        format!(
          "UPDATE {} SET obj = jsonb_set(obj, '{{{}}}', (COALESCE(obj->>'{}', '0')::int + $1)::text::jsonb) WHERE key = $2",
          coll, field, field
        )
        .as_str(),
        &[&amount, &key],
      ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
      }
    }))
  }

  fn drop_coll(&self, coll: String) -> Box<Future<Item = (), Error = DataError> + Send> {
    debug!("postgres data store drop coll: {}", coll);
    let pool = self.get_pool();
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
    )
    .as_str(),
    NO_PARAMS,
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;

  lazy_static! {
    static ref PG_URL: String = {
      format!(
        "postgres://{}@localhost:5432/{}",
        env::var("PG_TEST_USER").unwrap_or("postgres".to_string()),
        env::var("PG_TEST_DB").unwrap_or("postgres".to_string())
      )
    };
  }

  fn setup(dbname: Option<String>) -> PostgresDataStore {
    let conf = PostgresStoreConfig {
      url: (*PG_URL).clone(),
      database: dbname,
      tls_client_crt: None,
      tls_client_key: None,
      tls_ca_crt: None,
    };
    PostgresDataStore::new(&conf)
  }

  fn teardown(dbname: &str) {
    PG_POOLS
      .lock()
      .unwrap()
      .remove(&format!("{}:{}", *PG_URL, dbname));
    let conn = Connection::connect((*PG_URL).as_str(), postgres::TlsMode::None).unwrap();

    conn
      .execute(&format!("DROP DATABASE {}", dbname), NO_PARAMS)
      .unwrap();
  }

  fn set_value(store: &PostgresDataStore, coll: &str, key: &str, value: &str) {
    store
      .put(coll.to_string(), key.to_string(), value.to_string())
      .wait()
      .unwrap();
  }

  #[test]
  fn test_pg_some_dbname() {
    let dbname = "testflydata";
    let res: String = {
      let store = setup(Some(dbname.to_string()));
      let conn = store.get_pool().get().unwrap();

      conn
        .query(
          &format!(
            "SELECT datname FROM pg_database WHERE datname = '{}' LIMIT 1",
            &dbname
          ),
          NO_PARAMS,
        )
        .unwrap()
        .get(0)
        .get(0)
    };

    teardown(&dbname);

    assert_eq!(&res, dbname);
  }

  #[test]
  fn test_pg_put_get() {
    let dbname = "testflyputget";
    let coll = "coll1";
    let key = "test:key";
    let value = r#"{"foo":"bar"}"#;
    let got = {
      let store = setup(Some(dbname.to_string()));
      set_value(&store, coll, key, value);

      store
        .get(coll.to_string(), key.to_string())
        .wait()
        .unwrap()
        .unwrap()
    };

    teardown(&dbname);

    assert_eq!(
      serde_json::from_str::<serde_json::Value>(&got).unwrap(),
      serde_json::from_str::<serde_json::Value>(value).unwrap()
    );
  }

  #[test]
  fn test_pg_incr() {
    let dbname = "testflyincr";
    let coll = "coll1";
    let key = "test:key";
    let value = r#"{"counter":0,"foo":"bar"}"#;

    let store = setup(Some(dbname.to_string()));
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

    assert_eq!(got, "{\"foo\": \"bar\", \"counter\": 1}");

    store
      .incr(coll.to_string(), key.to_string(), "counter".to_string(), 15)
      .wait()
      .unwrap();
    let got = store
      .get(coll.to_string(), key.to_string())
      .wait()
      .unwrap()
      .unwrap();

    assert_eq!(got, "{\"foo\": \"bar\", \"counter\": 16}");

    teardown(&dbname);
  }

}
