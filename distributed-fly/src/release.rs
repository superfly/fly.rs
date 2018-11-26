use std::collections::HashMap;
use std::sync::RwLock;

use r2d2;
use rmps::Deserializer;
use serde::Deserialize;

use r2d2_redis::RedisConnectionManager;
use redis::Commands;

extern crate serde_json;

extern crate rmpv;
use self::rmpv::Value;

use kms::decrypt;

extern crate base64;

lazy_static! {
  static ref RELEASES: RwLock<HashMap<String, Release>> = RwLock::new(HashMap::new());
  static ref REDIS_POOL: r2d2::Pool<RedisConnectionManager> = r2d2::Pool::builder()
    .build(RedisConnectionManager::new("redis://localhost").unwrap())
    .unwrap();
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct Release {
  pub id: i32,
  pub app: String,
  pub app_id: i32,
  pub version: i32,
  pub files: Vec<String>,
  pub source: String,
  pub config: Value,
  pub secrets: Value,
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

  pub fn parsed_config(&self) -> Result<String, serde_json::Error> {
    let mut conf: Vec<(Value, Value)> = vec![];

    match self.config {
      Value::Map(ref map) => {
        for tup in map {
          conf.push(parse_config_entry(&tup, &self.secrets));
        }
      }
      _ => warn!("config is not a msgpack map..."),
    };

    serde_json::to_string(&Value::Map(conf))
  }
}

fn parse_config_entry(entry: &(Value, Value), secrets: &Value) -> (Value, Value) {
  if let Value::Map(ref map) = entry.1 {
    if map.len() == 1 {
      if let Value::String(ref name) = map[0].0 {
        if name.as_str() == Some("fromSecret") {
          if let Value::String(ref secret_utf8_name) = map[0].1 {
            if let Some(secret_name) = secret_utf8_name.as_str() {
              if let Some(plaintext) = get_secret(secrets, secret_name) {
                (entry.0.clone(), plaintext.into())
              } else {
                entry.clone()
              }
            } else {
              entry.clone()
            }
          } else {
            entry.clone()
          }
        } else {
          entry.clone()
        }
      } else {
        entry.clone()
      }
    } else {
      let mut new_entry: Vec<(Value, Value)> = Vec::with_capacity(map.len());
      for t in map {
        new_entry.push(parse_config_entry(t, secrets));
      }
      (entry.0.clone(), Value::Map(new_entry))
    }
  } else {
    entry.clone()
  }
  // if map.len() == 1 {
  //   if let Value::String(ref name) = map[0].0 {
  //     if name.as_str() == Some("fromSecret") {
  //       info!("{} is a secret! value: {}", k, map[0].1)

  //     }
  //   }
  // } else {
  //   for (k,v) in map {
  //     parse_config_map()
  //   }
  // }
}

fn get_secret<'a>(secrets: &'a Value, name: &str) -> Option<String> {
  match secrets {
    Value::Map(s) => {
      for (k, v) in s {
        if let Value::String(ks) = k {
          if let Some(sname) = ks.as_str() {
            if sname == name {
              if let Value::String(vs) = v {
                return match base64::decode(vs.as_bytes()) {
                  Ok(b64) => match String::from_utf8(decrypt(b64)) {
                    Ok(s) => Some(s),
                    Err(e) => {
                      error!("error decoding decrypted plaintext into string: {}", e);
                      None
                    }
                  },
                  Err(e) => {
                    error!("error decoding base64: {}", e);
                    None
                  }
                };
              }
            }
          }
        }
      }
      None
    }
    _ => None,
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
