use futures::{future, Future};

use crate::cache_store_notifier::*;

use crate::settings::RedisCacheNotifierConfig;

use r2d2_redis::RedisConnectionManager;
use r2d2_redis::{r2d2, redis};

use crate::redis_cache::purge_tag;

use std::time;

use crate::redis_pool::get_pool;
use std::collections::HashMap;
use std::sync::Mutex;

pub static CACHE_NOTIFIER_KEY: &str = "v2:notifier:cache";

lazy_static! {
    static ref REDIS_CACHE_NOTIFIERS: Mutex<HashMap<String, RedisCacheNotifier>> =
        Mutex::new(HashMap::new());
    static ref REDIS_CACHE_NOTIFIER_THREADS: Mutex<HashMap<String, JoinHandle<()>>> =
        Mutex::new(HashMap::new());
}

use std::thread::JoinHandle;

#[derive(Clone)]
pub struct RedisCacheNotifier {
    write_pool: r2d2::Pool<RedisConnectionManager>,
}

impl RedisCacheNotifier {
    pub fn new(conf: RedisCacheNotifierConfig, cache_url: String) -> Self {
        REDIS_CACHE_NOTIFIERS
            .lock()
            .unwrap()
            .entry(format!(
                "{}|{}|{}",
                conf.reader_url, conf.writer_url, cache_url
            ))
            .or_insert_with(move || {
                notification_listen(conf.reader_url, cache_url);
                RedisCacheNotifier {
                    write_pool: get_pool(conf.writer_url),
                }
            })
            .clone()
    }
}

impl CacheStoreNotifier for RedisCacheNotifier {
    fn notify(
        &self,
        op: CacheOperation,
        ns: String,
        value: String,
    ) -> Box<Future<Item = (), Error = CacheStoreNotifierError> + Send> {
        let pool = self.write_pool.clone();
        let ts = if let Ok(epoch) = time::SystemTime::now().duration_since(time::UNIX_EPOCH) {
            epoch.as_secs()
        } else {
            return Box::new(future::err(CacheStoreNotifierError::Failure(
                "redis notifier: could not get timestamp".to_string(),
            )));
        };
        let msg = CacheNotifyMessage {
            ns: ns,
            value: value,
            op: op,
        };
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(CacheStoreNotifierError::Failure(format!("{}", e))),
            Ok(conn) => match serde_json::to_string(&msg) {
                Err(e) => Err(CacheStoreNotifierError::Failure(format!("{}", e))),
                Ok(json) => match redis::cmd("ZADD")
                    .arg(CACHE_NOTIFIER_KEY)
                    .arg(ts)
                    .arg(json)
                    .query::<()>(&*conn)
                {
                    Err(e) => Err(CacheStoreNotifierError::Failure(format!("{}", e))),
                    Ok(_) => {
                        if let Err(e) = redis::cmd("ZREMRANGEBYSCORE")
                            .arg(CACHE_NOTIFIER_KEY)
                            .arg(0)
                            .arg(ts - 600)
                            .query::<()>(&*conn)
                        {
                            warn!("error removing old cache notifications: {}", e);
                        }
                        Ok(())
                    }
                },
            },
        }))
    }
}

use std::thread;

fn notification_listen(reader_url: String, cache_url: String) {
    REDIS_CACHE_NOTIFIER_THREADS
        .lock()
        .unwrap()
        .entry(format!("{}|{}", reader_url, cache_url))
        .or_insert_with(move || {
            let cpool = get_pool(cache_url);
            thread::Builder::new()
                .name("redis-notif-pubsub".to_string())
                .spawn(move || {
                    let client = redis::Client::open(reader_url.as_str()).unwrap();
                    // TODO: this appears to break pubsub... not sure why!
                    // match client.get_connection() {
                    //     Err(e) => {error!("could not get redis connection to check config: {}", e); return;},
                    //     Ok(conn) => match redis::cmd("CONFIG").arg("GET").arg("notify-keyspace-events").query::<Vec<String>>(&conn) {
                    //     Err(e) => {error!("could not get redis config: {}", e); return;},
                    //     Ok(confvec) => {
                    //         let mut conf = confvec[1].clone();
                    //         debug!("notify-keyspace-events value: {}", conf);
                    // if !conf.contains("E") || !conf.contains("A") && !conf.contains("z") {
                    //     conf = format!("{}{}", conf, "KEz");
                    //     info!("enabling keyspace notifications! value: {}", conf);
                    //     redis::cmd("CONFIG").arg("SET").arg("notify-keyspace-events").arg(conf).execute(&conn);
                    // }
                    //     }
                    // }
                    // };

                    let mut pubcon = client.get_connection().unwrap();

                    let mut pubsub = pubcon.as_pubsub();
                    if let Err(e) = pubsub.subscribe(format!("__keyspace@0__:{}", CACHE_NOTIFIER_KEY)) {
                        error!("error subscribing to global cache notifications: {}", e);
                        return;
                    }
                    info!("subscribed to global cache notifications");
                    let mut last_updated_at = time::SystemTime::now()
                        .duration_since(time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    loop {
                        info!("waiting for message");
                        let msg = pubsub.get_message().unwrap();
                        let payload: String = msg.get_payload().unwrap();
                        info!("channel '{}': {}", msg.get_channel_name(), payload);
                        let now = time::SystemTime::now()
                            .duration_since(time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        let conn = client.get_connection().unwrap();
                        match redis::cmd("ZRANGEBYSCORE")
                            .arg(CACHE_NOTIFIER_KEY)
                            .arg(last_updated_at)
                            .arg(now)
                            .query::<Vec<String>>(&conn)
                        {
                            Err(e) => error!("error getting cache notifications range: {}", e),
                            Ok(notifications) => {
                                for n in notifications.iter() {
                                    match serde_json::from_str::<CacheNotifyMessage>(n.as_str()) {
                                        Err(e) => {
                                            error!("could not parse cache notification: {}", e)
                                        }
                                        Ok(notif) => match notif.op {
                                            CacheOperation::Del => match cpool.get() {
                                                Ok(cconn) => {
                                                    debug!(
                                                        "cache notification delete key: {}",
                                                        notif.value
                                                    );
                                                    redis::cmd("DEL")
                                                        .arg(notif.value)
                                                        .execute(&*cconn);
                                                }
                                                Err(e) => error!(
                                                    "could not acquire cache connection from pool: {}",
                                                    e
                                                ),
                                            },
                                            CacheOperation::PurgeTag => match cpool.get() {
                                                Ok(cconn) => {
                                                    debug!(
                                                        "cache notification purge tag: {}",
                                                        notif.value
                                                    );
                                                    if let Err(e) =
                                                        purge_tag(&*cconn, notif.value.clone())
                                                    {
                                                        error!(
                                                            "error purging tag '{}': {}",
                                                            notif.value, e
                                                        );
                                                    }
                                                }
                                                Err(e) => error!(
                                                    "could not acquire cache connection from pool: {}",
                                                    e
                                                ),
                                            },
                                        },
                                    }
                                }
                                last_updated_at = now;
                            }
                        };
                    }
                })
                .unwrap()
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> RedisCacheNotifier {
        redis::cmd("CONFIG")
            .arg("SET")
            .arg("notify-keyspace-events")
            .arg("KEA")
            .execute(&redis::Client::open("redis://localhost:6379").unwrap());

        RedisCacheNotifier::new(
            RedisCacheNotifierConfig {
                writer_url: "redis://localhost:6379".to_string(),
                reader_url: "redis://localhost:6379".to_string(),
            },
            "redis://localhost:6379".to_string(),
        )
    }

    fn redis_conn() -> redis::Connection {
        redis::Client::open("redis://localhost:6379")
            .unwrap()
            .get_connection()
            .unwrap()
    }

    #[test]
    fn test_redis_cache_notifier_notify() {
        // let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
        // env_logger::init_from_env(env);

        let store = setup();

        let conn = store.write_pool.get().unwrap();
        redis::cmd("DEL")
            .arg(CACHE_NOTIFIER_KEY)
            .query::<()>(&*conn)
            .unwrap();

        let key = "testtest";

        redis::cmd("SET")
            .arg(key)
            .arg("hello world")
            .query::<()>(&*conn)
            .unwrap();

        let testns = "testns".to_string();

        let handle = thread::spawn(move || {
            let mut conn_for_pubsub = redis_conn();
            let mut pconn = conn_for_pubsub.as_pubsub();
            pconn
                .subscribe(format!("__keyspace@0__:{}", key).as_str())
                .unwrap();
            pconn.get_message().unwrap();
        });

        thread::sleep(std::time::Duration::from_millis(10)); // unfortunate, but might be enough...

        let res = store
            .notify(CacheOperation::Del, testns.clone(), key.to_string())
            .wait()
            .unwrap();

        assert_eq!(res, ());

        handle.join().unwrap(); // block until something happens on our key, such as DEL!

        let pushed = redis::cmd("ZREVRANGE")
            .arg(CACHE_NOTIFIER_KEY)
            .arg(0)
            .arg(-1)
            .query::<Vec<String>>(&*conn)
            .unwrap();

        let first: CacheNotifyMessage = serde_json::from_str(&pushed[0]).unwrap();

        assert_eq!(first.value, key);
        assert_eq!(first.ns, testns);
        assert_eq!(first.op, CacheOperation::Del);

        assert!(redis::cmd("GET")
            .arg(key)
            .query::<Option<String>>(&*conn)
            .unwrap()
            .is_none());
    }
}
