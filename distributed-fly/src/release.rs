use std::collections::HashMap;
use std::sync::RwLock;

use r2d2;
use rmps::Deserializer;
use serde::Deserialize;

use r2d2_redis::RedisConnectionManager;
use redis::Commands;

lazy_static! {
  static ref RELEASES: RwLock<HashMap<String, Release>> = RwLock::new(HashMap::new());
  static ref REDIS_POOL: r2d2::Pool<RedisConnectionManager> = r2d2::Pool::builder()
    .build(RedisConnectionManager::new("redis://localhost").unwrap())
    .unwrap();
}

#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
pub struct Release {
  pub id: i32,
  pub app: String,
  pub app_id: i32,
  pub version: i32,
  pub files: Vec<String>,
  pub source: String,
}

impl Release {
  pub fn get(host: &str) -> Result<Option<Release>, String> {
    match find_cached_release(host) {
      Some(r) => Ok(Some(r)),
      None => {
        let conn = match REDIS_POOL.get() {
          Ok(c) => c,
          Err(e) => return Err(format!("{}", e)),
        };
        let k: String = match conn.hget("app_hosts", host) {
          Ok(s) => s,
          Err(e) => return Err(format!("{}", e)),
        };
        if k.is_empty() {
          // return future::ok(
          //   Response::builder()
          //     .status(StatusCode::NOT_FOUND)
          //     .body(Body::from("app not found"))
          //     .unwrap(),
          // );
          return Ok(None);
        }
        info!("got key! {}", k);
        let buf: Vec<u8> = match conn.get(format!("{}:release:latest", k)) {
          Ok(v) => v,
          Err(e) => return Err(format!("{}", e)),
        };
        if buf.is_empty() {
          // TODO: Maybe this should be an error
          return Ok(None);
          // return future::ok(
          //   Response::builder()
          //     .status(StatusCode::NOT_FOUND)
          //     .body(Body::from("release not found"))
          //     .unwrap(),
          // );
        }
        let mut de = Deserializer::new(&buf[..]);
        match Deserialize::deserialize(&mut de) {
          Ok(rel) => {
            let rel: Release = rel; // type annotation required at the moment.
            RELEASES
              .write()
              .unwrap()
              .insert(host.to_string(), rel.clone());
            Ok(Some(rel))
          }
          Err(e) => return Err(format!("{}", e)),
        }
      }
    }
  }
}

fn find_cached_release(host: &str) -> Option<Release> {
  match RELEASES.read() {
    Err(e) => {
      error!("error acquiring lock on releases cache: {}", e);
      None
    }
    Ok(guard) => match guard.get(host) {
      Some(r) => Some(r.clone()),
      None => None,
    },
  }
}
