use std::collections::HashMap;
use std::sync::RwLock;

use rmp_serde::Deserializer;
use serde::Deserialize;

use crate::is_interrupting;
use r2d2_redis::redis::{self, Commands};

use rmpv::Value;

use crate::kms::decrypt;

use crate::settings::GLOBAL_SETTINGS;

use super::REDIS_POOL;
use std::thread;

lazy_static! {
  static ref RELEASES_BY_APP: RwLock<HashMap<String, Release>> = RwLock::new(HashMap::new());
  static ref APP_BY_HOSTNAME: RwLock<HashMap<String, String>> = RwLock::new(HashMap::new());
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
  pub libs: Option<Vec<String>>,
}

impl Release {
  pub fn get(host: &str) -> Result<Option<Release>, String> {
    match find_cached_release(host) {
      Some(r) => Ok(Some(r)),
      None => {
        let conn = match REDIS_POOL.get() {
          Ok(c) => c,
          Err(e) => return Err(format!("error getting pool connection: {}", e)),
        };
        let app_key: String = match redis::cmd("HGET")
          .arg("app_hosts")
          .arg(host)
          .query::<Option<String>>(&*conn)
        {
          Ok(s) => match s {
            Some(s) => s,
            None => return Ok(None),
          },
          Err(e) => return Err(format!("error getting app host: {}", e)),
        };
        if app_key.is_empty() {
          return Ok(None);
        }
        info!("got key! {}", app_key);
        {
          match APP_BY_HOSTNAME.write() {
            Ok(mut w) => {
              w.insert(host.to_string(), app_key.clone());
              debug!("inserted {} => {} in app_by_hostname", host, app_key);
            }
            Err(e) => error!("could not acquire app_by_hostname lock: {}", e),
          }
        }
        match get_by_app_key(&*conn, app_key.as_str())? {
          Some(rel) => {
            let mut w = match RELEASES_BY_APP.write() {
              Ok(w) => w,
              Err(e) => {
                error!("poisoned RELEASES_BY_APP lock! {}", e);
                e.into_inner()
              }
            };

            w.insert(app_key, rel.clone());
            Ok(Some(rel))
          }
          None => Ok(None),
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

fn get_by_app_key(conn: &redis::Connection, app_key: &str) -> Result<Option<Release>, String> {
  let buf: Vec<u8> = match conn.get(format!("{}:release:latest", app_key)) {
    Ok(v) => v,
    Err(e) => return Err(format!("{}", e)),
  };
  if buf.is_empty() {
    // TODO: Maybe this should be an error
    return Ok(None);
  }
  let mut de = Deserializer::new(&buf[..]);
  match Deserialize::deserialize(&mut de) {
    Ok(rel) => Ok(Some(rel)),
    Err(e) => return Err(format!("{}", e)),
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
                  Ok(bytes) => match decrypt(bytes) {
                    Err(e) => {
                      error!("error decrypting secret: {}", e);
                      None
                    }
                    Ok(maybe_plain) => match maybe_plain {
                      None => None,
                      Some(plain) => match String::from_utf8(plain) {
                        Ok(s) => Some(s),
                        Err(e) => {
                          error!("error decoding decrypted plaintext into string: {}", e);
                          None
                        }
                      },
                    },
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
  match APP_BY_HOSTNAME.read() {
    Err(e) => {
      error!("error acquiring lock on app key cache: {}", e);
      None
    }
    Ok(guard) => match guard.get(host) {
      Some(key) => match RELEASES_BY_APP.read() {
        Err(e) => {
          error!("error acquiring lock on releases cache: {}", e);
          None
        }
        Ok(r) => match r.get(key) {
          Some(r) => Some(r.clone()),
          None => None,
        },
      },
      None => None,
    },
  }
}

use std::time;

fn sleep_a_bit() {
  thread::sleep(time::Duration::from_secs(5)); // probably want to do backoff later
}

pub fn start_new_release_check() {
  thread::Builder::new()
    .name("redis-release-pubsub".to_string())
    .spawn(|| {
      let mut last_updated_at = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
      while !is_interrupting() {
        let redis_url = { GLOBAL_SETTINGS.read().unwrap().redis_url.clone() };
        let client = match redis::Client::open(redis_url.as_str()) {
          Ok(c) => c,
          Err(e) => {
            error!("unable to connect to redis for release checking: {}", e);
            sleep_a_bit();
            continue;
          }
        };
        let mut con = match client.get_connection() {
          Ok(c) => c,
          Err(e) => {
            error!(
              "unable to get the redis connection for release checking: {}",
              e
            );
            sleep_a_bit();
            continue;
          }
        };
        let mut pubsub = con.as_pubsub();
        match pubsub.subscribe("__keyspace@0__:notifications") {
          Ok(_) => {}
          Err(e) => {
            error!("unable to subscribe to redis keyspace notifications: {}", e);
            sleep_a_bit();
            continue;
          }
        };
        info!("subscribed to keyspace notifications");

        while !is_interrupting() {
          let msg = match pubsub.get_message() {
            Ok(m) => m,
            Err(e) => {
              error!("error getting message from pubsub: {}", e);
              sleep_a_bit();
              continue;
            }
          };
          let payload: String = match msg.get_payload() {
            Ok(p) => p,
            Err(e) => {
              error!("error getting payload from pubsub message: {}", e);
              sleep_a_bit();
              continue;
            }
          };
          info!("channel '{}': {}", msg.get_channel_name(), payload);
          let now = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
          match REDIS_POOL.get() {
            Err(e) => error!("could not acquire redis conn: {}", e),
            Ok(conn) => match redis::cmd("ZRANGEBYSCORE")
              .arg("notifications")
              .arg(last_updated_at)
              .arg(now)
              .query::<Vec<String>>(&*conn)
            {
              Err(e) => error!("error getting notifications range: {}", e),
              Ok(notifications) => {
                for n in notifications.iter() {
                  match serde_json::from_str::<ReleaseNotification>(n.as_str()) {
                    Err(e) => error!("could not parse notification: {}", e),
                    Ok(notif) => {
                      use self::NotificationAction::*;
                      if notif.key.starts_with("app:") {
                        match notif.action {
                          Delete => match RELEASES_BY_APP.write() {
                            Ok(mut guard) => {
                              guard.remove(notif.key.as_str());
                            }
                            Err(e) => error!("error getting RELEASES_BY_APP write lock: {}", e),
                          },
                          Update => match get_by_app_key(&*conn, notif.key.as_str()) {
                            Ok(maybe_rel) => match maybe_rel {
                              Some(rel) => match RELEASES_BY_APP.write() {
                                Ok(mut guard) => {
                                  guard.insert(notif.key.clone(), rel);
                                }
                                Err(e) => error!("error getting RELEASES_BY_APP write lock: {}", e),
                              },
                              None => {}
                            },
                            Err(e) => error!("error getting app by key: {}", e),
                          },
                        }
                      } else if notif.key.starts_with("app_hosts") {
                        use serde_json::Value;
                        match notif.context {
                          Value::Array(arr) => {
                            let hostnames: Vec<String> = arr
                              .iter()
                              .map(|v| match v {
                                Value::String(h) => h.clone(),
                                _ => unimplemented!(),
                              })
                              .collect();
                            match notif.action {
                              Delete => match APP_BY_HOSTNAME.write() {
                                Ok(mut guard) => {
                                  for h in hostnames.iter() {
                                    guard.remove(h);
                                  }
                                }
                                Err(e) => {
                                  error!("error acquiring APP_BY_HOSTNAME write lock: {}", e)
                                }
                              },
                              Update => {
                                for h in hostnames.iter() {
                                  match redis::cmd("HGET")
                                    .arg("app_hosts")
                                    .arg(h)
                                    .query::<Option<String>>(&*conn)
                                  {
                                    Ok(Some(app_key)) => match APP_BY_HOSTNAME.write() {
                                      Ok(mut guard) => {
                                        guard.insert(h.clone(), app_key);
                                      }
                                      Err(e) => {
                                        error!("error acquiring APP_BY_HOSTNAME write lock: {}", e)
                                      }
                                    },
                                    Ok(None) => debug!("no app host found for {}", h),
                                    Err(e) => error!("could not get app_hosts for {}: {}", h, e),
                                  }
                                }
                              }
                            }
                          }
                          _ => unimplemented!(),
                        }
                      }
                    }
                  }
                }
                last_updated_at = now;
              }
            },
          };
        }
      }
    })
    .unwrap();
}

#[derive(Debug, Deserialize)]
struct ReleaseNotification {
  action: NotificationAction,
  key: String,
  context: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum NotificationAction {
  Delete,
  Update,
}
