use data::*;
use std::sync::Arc;

extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;
use self::r2d2_postgres::{PostgresConnectionManager, TlsMode};

use self::postgres::params::{Builder, ConnectParams, IntoConnectParams};
use self::postgres::tls::openssl::openssl::ssl::{SslConnectorBuilder, SslMethod};
use self::postgres::tls::openssl::openssl::x509::{X509_FILETYPE_DEFAULT, X509_FILETYPE_PEM};
use self::postgres::types::ToSql;
use self::postgres::Connection;

use settings::PostgresStoreConfig;

extern crate serde_json;

use futures::{future, Future};

pub struct PostgresDataStore {
  pool: Arc<r2d2::Pool<PostgresConnectionManager>>,
}

impl PostgresDataStore {
  pub fn new(conf: &PostgresStoreConfig) -> Self {
    let url = conf.url.clone();
    let maybe_dbname = conf.database.as_ref().cloned();
    let params: ConnectParams = url.into_connect_params().unwrap();
    let mut builder = Builder::new();
    builder.port(params.port());
    if let Some(user) = params.user() {
      builder.user(user.name(), user.password());
    }

    if let Some(dbname) = params.database() {
      builder.database(dbname);
    }

    let params = builder.build(params.host().clone());

    let maybe_tls = if conf.tls_client_crt.is_some() {
      let mut connbuilder = SslConnectorBuilder::new(SslMethod::tls()).unwrap();
      if let Some(ref ca) = conf.tls_ca_crt {
        connbuilder.set_ca_file(ca).unwrap();
      }
      connbuilder
        .set_certificate_file(conf.tls_client_crt.as_ref().unwrap(), X509_FILETYPE_DEFAULT)
        .unwrap();
      connbuilder
        .set_private_key_file(conf.tls_client_key.as_ref().unwrap(), X509_FILETYPE_PEM)
        .unwrap();
      // connbuilder.
      // connbuilder.set_verify(postgres::tls::openssl::openssl::ssl::);
      Some(postgres::tls::openssl::OpenSsl::from(connbuilder.build()))
    } else {
      None
    };

    let pool = if let Some(dbname) = &maybe_dbname {
      let conn = Connection::connect(
        params.clone(),
        match maybe_tls.as_ref() {
          Some(tls) => postgres::TlsMode::Require(tls),
          None => postgres::TlsMode::None,
        },
      ).unwrap();
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
      let manager = PostgresConnectionManager::new(
        pool_params,
        match maybe_tls {
          Some(tls) => TlsMode::Require(Box::new(tls)),
          None => TlsMode::None,
        },
      ).unwrap();
      r2d2::Pool::builder().build(manager).unwrap()
    } else {
      let manager = PostgresConnectionManager::new(
        params,
        match maybe_tls {
          Some(tls) => TlsMode::Require(Box::new(tls)),
          None => TlsMode::None,
        },
      ).unwrap();
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
    let conn = Connection::connect((*PG_URL).as_str(), postgres::TlsMode::None).unwrap();

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
