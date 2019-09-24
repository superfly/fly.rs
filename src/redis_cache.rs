use futures::{future, stream, Future, Stream};

use crate::cache_store::*;

extern crate r2d2_redis;

use crate::settings::{RedisCacheNotifierConfig, RedisStoreConfig};

use std::fmt::Display;
use std::time;

use std::collections::HashMap;

use crate::metrics::*;

use std::sync::Arc;

use prometheus::{Histogram, IntCounter};

use crate::cache_store_notifier::{CacheOperation, CacheStoreNotifier, CacheStoreNotifierError};
use crate::redis_cache_notifier::RedisCacheNotifier;

use crate::redis_pool::get_pool;

lazy_static! {
    static ref REPLACE_HASH: redis::Script = redis::Script::new(
        r#"
    local key = KEYS[1]
    local res = redis.call("DEL", key)
    local ttl = table.remove(ARGV, 1)
    if table.getn(ARGV) > 0 then
      redis.call("HMSET", key, unpack(ARGV))
    end
    if tonumber(ttl) > 0 then
      return redis.call("EXPIRE", key, ttl)
    end
    return res
  "#
    );
    static ref SET_TAGS: redis::Script = {
        let src = format!(
            r#"
      local key = KEYS[1]
      local ts = table.remove(ARGV, 1)

      local res = redis.call("HSET", key, "ts", ts) -- update timestamp

      for i, tag in ipairs(ARGV) do
        redis.call("HSET", key, "{}:"..tag, 1)
      end

      return res
    "#,
            TAG_PREFIX
        );
        redis::Script::new(src.as_str())
    };
    static ref GET_CACHE: redis::Script = redis::Script::new(
        r#"
    local key = KEYS[1]
    local typ = redis.call("TYPE", key)
    if typ and typ.ok ~= "hash" then
      -- redis.log(redis.LOG_WARNING, "deprecated key type: "..typ.ok)
      redis.call("DEL", key)
      return {0, ""} -- init reply
    end
    -- redis.log(redis.LOG_NOTICE, "key type was up to date")

    return redis.call("HMGET", key, "ts", "meta")
  "#
    );
}

use self::r2d2_redis::RedisConnectionManager;
use self::r2d2_redis::{r2d2, redis};

static CACHE_PREFIX: &str = "v2:cache";
static TAG_PREFIX: &str = "v2:tag";

pub struct RedisCacheStore {
    pool: r2d2::Pool<RedisConnectionManager>,
    ns: String,
    notifier: Option<RedisCacheNotifier>,
    metric_get_duration: Histogram,
    metric_set_duration: Histogram,
    metric_hits_total: IntCounter,
    metric_misses_total: IntCounter,
    metric_errors_total: IntCounter,
    metric_gets_total: IntCounter,
    metric_get_size_total: IntCounter,
    metric_sets_total: IntCounter,
    metric_set_size_total: IntCounter,
    metric_dels_total: IntCounter,
    metric_expires_total: IntCounter,
    metric_ttls_total: IntCounter,
    metric_set_tags_total: IntCounter,
}

impl RedisCacheStore {
    pub fn new(conf: &RedisStoreConfig, notifier_conf: Option<RedisCacheNotifierConfig>) -> Self {
        let ns = conf.namespace.as_ref().cloned().unwrap_or("".to_string());
        let ns_str = ns.as_str();

        RedisCacheStore {
            pool: get_pool(conf.url.clone()),
            ns: ns.clone(),
            notifier: match notifier_conf {
                None => None,
                Some(csnconf) => Some(RedisCacheNotifier::new(csnconf, conf.url.clone())),
            },
            metric_get_duration: CACHE_GET_DURATION.with_label_values(&["redis", ns_str]),
            metric_set_duration: CACHE_SET_DURATION.with_label_values(&["redis", ns_str]),
            metric_hits_total: CACHE_HITS_TOTAL.with_label_values(&["redis", ns_str]),
            metric_misses_total: CACHE_MISSES_TOTAL.with_label_values(&["redis", ns_str]),
            metric_errors_total: CACHE_ERRORS_TOTAL.with_label_values(&["redis", ns_str]),
            metric_gets_total: CACHE_GETS_TOTAL.with_label_values(&["redis", ns_str]),
            metric_get_size_total: CACHE_GET_SIZE_TOTAL.with_label_values(&["redis", ns_str]),
            metric_sets_total: CACHE_SETS_TOTAL.with_label_values(&["redis", ns_str]),
            metric_set_size_total: CACHE_SET_SIZE_TOTAL.with_label_values(&["redis", ns_str]),
            metric_dels_total: CACHE_DELS_TOTAL.with_label_values(&["redis", ns_str]),
            metric_expires_total: CACHE_EXPIRES_TOTAL.with_label_values(&["redis", ns_str]),
            metric_ttls_total: CACHE_TTLS_TOTAL.with_label_values(&["redis", ns_str]),
            metric_set_tags_total: CACHE_SET_TAGS_TOTAL.with_label_values(&["redis", ns_str]),
        }
    }

    fn cache_key<S: Display>(&self, key: S) -> String {
        format!("{}:{}:{}", CACHE_PREFIX, self.ns, key)
    }

    fn tag_key<S: Display>(&self, tag: S) -> String {
        format!("{}:{}:{}", TAG_PREFIX, self.ns, tag)
    }
}

impl CacheStore for RedisCacheStore {
    fn set(
        &self,
        key: String,
        data: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
        opts: CacheSetOptions,
    ) -> EmptyCacheFuture {
        self.metric_sets_total.inc();
        let fullkey = self.cache_key(key);
        let cfullkey = fullkey.clone();

        let ns = self.ns.clone();
        let timer = self.metric_set_duration.start_timer();
        let size_metric = self.metric_set_size_total.clone();

        let pool = self.pool.clone();
        let cpool = pool.clone();

        Box::new(
            future::lazy(move || match pool.get() {
                Ok(conn) => {
                    let ts = time::SystemTime::now()
                        .duration_since(time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let mut script = REPLACE_HASH.key(&fullkey);
                    script.arg(opts.ttl.unwrap_or(0));
                    script.arg("ts").arg(ts);

                    if let Some(meta) = opts.meta {
                        script.arg("meta").arg(meta);
                    }

                    match script.invoke::<()>(&*conn) {
                        Ok(_) => {
                            if let Some(tags) = opts.tags {
                                for tag in tags.iter() {
                                    match redis::cmd("ZADD")
                                        .arg(format!("{}:{}:{}", TAG_PREFIX, ns, tag))
                                        .arg(ts)
                                        .arg(&fullkey)
                                        .query::<()>(&*conn)
                                    {
                                        Ok(_) => {}
                                        Err(e) => {
                                            return Err(CacheError::Failure(format!("{}", e)))
                                        }
                                    }
                                }
                            }
                            Ok(())
                        }
                        Err(e) => Err(CacheError::Failure(format!("{}", e))),
                    }
                }
                Err(e) => Err(CacheError::Failure(format!("{}", e))),
            })
            .and_then(move |_| {
                data.map_err(|_| CacheError::Unknown)
                    .fold(0, move |idx, chunk| match cpool.get() {
                        Ok(conn) => match redis::cmd("HSET")
                            .arg(&cfullkey)
                            .arg(format!("chunk:{}", idx))
                            .arg(chunk.as_slice())
                            .query::<()>(&*conn)
                        {
                            Ok(_) => {
                                size_metric.inc_by(chunk.len() as i64);
                                Ok(idx + 1)
                            }
                            Err(e) => Err(CacheError::Failure(format!("{}", e))),
                        },
                        Err(e) => Err(CacheError::Failure(format!("{}", e))),
                    })
                    .and_then(move |_| {
                        timer.observe_duration();
                        Ok(())
                    })
            }),
        )
    }

    fn get(
        &self,
        key: String,
    ) -> Box<Future<Item = Option<CacheEntry>, Error = CacheError> + Send> {
        self.metric_gets_total.inc();
        let fullkey = self.cache_key(key);
        debug!("redis cache get with key: {}", fullkey);

        let timer = Arc::new(self.metric_get_duration.start_timer());
        let size_metric = self.metric_get_size_total.clone();
        let metric_misses = self.metric_misses_total.clone();
        let metric_hits = self.metric_hits_total.clone();
        let metric_errors = self.metric_errors_total.clone();

        let pool = self.pool.clone();

        Box::new(future::lazy(move || match pool.get() {
            Ok(conn) => match GET_CACHE
                .key(&fullkey)
                .invoke::<(i32, Option<String>)>(&*conn)
            {
                Ok(vals) => {
                    if vals.0 == 0 {
                        metric_misses.inc();
                        return Ok(None);
                    }
                    metric_hits.inc();
                    Ok(Some(CacheEntry {
                        meta: vals.1,
                        stream: Box::new(stream::unfold(0, move |idx| match pool.get() {
                            Ok(conn) => match redis::cmd("HGET")
                                .arg(&fullkey)
                                .arg(format!("chunk:{}", idx))
                                .query::<Vec<u8>>(&*conn)
                            {
                                Ok(r) => {
                                    if r.len() == 0 {
                                        return None;
                                    }
                                    size_metric.inc_by(r.len() as i64);
                                    let _t = timer.clone(); // keep it alive.
                                    Some(future::ok((r, idx + 1)))
                                }
                                Err(e) => {
                                    metric_errors.inc();
                                    Some(future::err(CacheError::Failure(format!("{}", e))))
                                }
                            },
                            Err(e) => {
                                metric_errors.inc();
                                Some(future::err(CacheError::Failure(format!("{}", e))))
                            }
                        })),
                    }))
                }
                Err(e) => {
                    metric_errors.inc();
                    Err(CacheError::Failure(format!("{}", e)))
                }
            },
            Err(e) => {
                metric_errors.inc();
                Err(CacheError::Failure(format!("{}", e)))
            }
        }))
    }

    fn del(&self, key: String) -> EmptyCacheFuture {
        self.metric_dels_total.inc();
        let fullkey = self.cache_key(key);
        debug!("redis cache del key: {}", fullkey);
        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
            Ok(conn) => match redis::cmd("DEL").arg(&fullkey).query::<()>(&*conn) {
                Err(e) => Err(CacheError::Failure(format!("{}", e))),
                Ok(_) => Ok(()),
            },
        }))
    }

    fn expire(&self, key: String, ttl: u32) -> EmptyCacheFuture {
        self.metric_expires_total.inc();
        let fullkey = self.cache_key(key);
        debug!("redis cache expire key: {} w/ ttl: {}", fullkey, ttl);

        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
            Ok(conn) => match redis::cmd("EXPIRE")
                .arg(&fullkey)
                .arg(ttl)
                .query::<()>(&*conn)
            {
                Err(e) => Err(CacheError::Failure(format!("{}", e))),
                Ok(_) => Ok(()),
            },
        }))
    }

    fn ttl(&self, key: String) -> Box<Future<Item = i32, Error = CacheError> + Send> {
        self.metric_ttls_total.inc();
        let fullkey = self.cache_key(key);
        debug!("redis cache ttl key: {}", fullkey);

        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
            Ok(conn) => match redis::cmd("TTL").arg(&fullkey).query::<i32>(&*conn) {
                Err(e) => Err(CacheError::Failure(format!("{}", e))),
                Ok(i) => Ok(i),
            },
        }))
    }

    fn purge_tag(&self, tag: String) -> EmptyCacheFuture {
        CACHE_PURGES_TOTAL
            .with_label_values(&["redis", self.ns.as_str(), tag.as_str()])
            .inc(); // can't do that oen statically.
        debug!("redis cache purge_tag tag: {}", tag);
        let tagkey = self.tag_key(tag);
        let pool = self.pool.clone();

        Box::new(future::lazy(move || match pool.get() {
            Ok(conn) => match purge_tag(&*conn, tagkey) {
                Ok(_) => Ok(()),
                Err(e) => Err(CacheError::Failure(format!("{}", e))),
            },
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
        }))
    }

    fn set_tags(&self, key: String, tags: Vec<String>) -> EmptyCacheFuture {
        self.metric_set_tags_total.inc();
        debug!("redis cache set tags key: {}, tags: {:?}", key, tags);
        let fullkey = self.cache_key(&key);

        let ns = self.ns.clone();
        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Ok(conn) => {
                let ts = time::SystemTime::now()
                    .duration_since(time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let mut script = SET_TAGS.key(&fullkey);
                script.arg(ts);
                for tag in tags.iter() {
                    script.arg(tag);
                }
                match script.invoke::<()>(&*conn) {
                    Ok(_) => {
                        for tag in tags.iter() {
                            match redis::cmd("ZADD")
                                .arg(format!("{}:{}:{}", TAG_PREFIX, ns, tag))
                                .arg(ts)
                                .arg(&fullkey)
                                .query::<()>(&*conn)
                            {
                                Ok(_) => {}
                                Err(e) => return Err(CacheError::Failure(format!("{}", e))),
                            }
                        }
                        Ok(())
                    }
                    Err(e) => Err(CacheError::Failure(format!("{}", e))),
                }
            }
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
        }))
    }

    fn notify(
        &self,
        op: CacheOperation,
        value: String,
    ) -> Box<Future<Item = (), Error = CacheStoreNotifierError> + Send> {
        if self.notifier.is_none() {
            return Box::new(future::err(CacheStoreNotifierError::Unavailable));
        }

        let notifier = self.notifier.as_ref().unwrap();

        notifier.notify(op, self.ns.clone(), value)
    }

    fn set_meta(&self, key: String, meta: String) -> EmptyCacheFuture {
        self.metric_ttls_total.inc();
        let fullkey = self.cache_key(key);
        debug!("redis cache set_meta key: {}", fullkey);

        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(CacheError::Failure(format!("{}", e))),
            Ok(conn) => match redis::cmd("HSET")
                .arg(&fullkey)
                .arg("meta")
                .arg(meta)
                .query::<()>(&*conn)
            {
                Err(e) => Err(CacheError::Failure(format!("{}", e))),
                Ok(_) => Ok(()),
            },
        }))
    }
}

pub fn purge_tag(conn: &redis::Connection, tagkey: String) -> Result<(), redis::RedisError> {
    let keysts = redis::cmd("ZRANGE")
        .arg(&tagkey)
        .arg(0)
        .arg(-1)
        .arg("WITHSCORES")
        .query::<HashMap<String, i32>>(conn)?;
    for (key, tagts) in keysts.iter() {
        let ref ts = redis::cmd("HGET").arg(key).arg("ts").query::<i32>(conn)?;
        if ts == tagts {
            redis::cmd("DEL").arg(key).execute(conn);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate rand;
    use self::rand::{thread_rng, RngCore};

    fn setup() -> RedisCacheStore {
        RedisCacheStore::new(
            &RedisStoreConfig {
                url: "redis://localhost:6379".to_string(),
                namespace: Some("test".to_string()),
            },
            None,
        )
    }

    fn set_value(
        store: &RedisCacheStore,
        key: &str,
        v: &[u8],
        opts: CacheSetOptions,
    ) -> CacheResult<()> {
        let size = 256;
        let value = v.to_vec();
        store
            .set(
                key.to_string(),
                Box::new(stream::unfold(0, move |pos| {
                    if pos >= value.len() {
                        return None;
                    }
                    let end = pos + size;
                    if end > value.len() {
                        Some(future::ok((value[pos..].to_vec(), end)))
                    } else {
                        Some(future::ok((value[pos..end].to_vec(), end)))
                    }
                })),
                opts,
            )
            .wait()
    }

    #[test]
    fn test_redis_cache_set() {
        let store = setup();
        let mut v = [0u8; 1000];
        thread_rng().fill_bytes(&mut v);
        let key = "testset";
        set_value(
            &store,
            key,
            &v,
            CacheSetOptions {
                tags: None,
                ttl: None,
                meta: None,
            },
        )
        .unwrap();

        let entry = store.get(key.to_string()).wait().unwrap().unwrap();
        assert_eq!(v.to_vec(), entry.stream.concat2().wait().unwrap());

        let conn = store.pool.get().unwrap();

        assert!(redis::cmd("HEXISTS")
            .arg(store.cache_key(key))
            .arg("ts")
            .query::<bool>(&*conn)
            .unwrap());
    }

    #[test]
    fn test_redis_cache_set_w_tags() {
        let store = setup();
        let mut v = [0u8; 1000];
        thread_rng().fill_bytes(&mut v);
        let key = "testsetwtags";
        let tags = vec!["foo".to_string(), "bar".to_string()];

        set_value(
            &store,
            key,
            &v,
            CacheSetOptions {
                tags: Some(tags.clone()),
                ttl: None,
                meta: None,
            },
        )
        .unwrap();

        let entry = store.get(key.to_string()).wait().unwrap().unwrap();
        assert_eq!(v.to_vec(), entry.stream.concat2().wait().unwrap());

        let conn = store.pool.get().unwrap();

        let fullkey = store.cache_key(key);
        let ts = redis::cmd("HGET")
            .arg(&fullkey)
            .arg("ts")
            .query::<i32>(&*conn)
            .unwrap();

        for tag in tags.iter() {
            assert_eq!(
                ts,
                redis::cmd("ZSCORE")
                    .arg(store.tag_key(tag))
                    .arg(&fullkey)
                    .query::<i32>(&*conn)
                    .unwrap()
            );
        }
    }

    #[test]
    fn test_redis_cache_set_w_meta() {
        let store = setup();
        let mut v = [0u8; 1000];
        thread_rng().fill_bytes(&mut v);
        let key = "testsetwmeta";
        let meta = "foobar";

        set_value(
            &store,
            key,
            &v,
            CacheSetOptions {
                tags: None,
                ttl: None,
                meta: Some(meta.to_string()),
            },
        )
        .unwrap();

        let entry = store.get(key.to_string()).wait().unwrap().unwrap();
        assert_eq!(v.to_vec(), entry.stream.concat2().wait().unwrap());

        let conn = store.pool.get().unwrap();

        assert_eq!(
            redis::cmd("HGET")
                .arg(store.cache_key(key))
                .arg("meta")
                .query::<String>(&*conn)
                .unwrap(),
            meta
        );
    }

    #[test]
    fn test_redis_cache_purge_tags() {
        let store = setup();
        let mut v = [0u8; 1000];
        thread_rng().fill_bytes(&mut v);
        let key1 = "testpurge1";
        let key2 = "testpurge2";

        set_value(
            &store,
            key1,
            &v,
            CacheSetOptions {
                tags: Some(vec!["tag1".to_string(), "tag2".to_string()]),
                ttl: None,
                meta: None,
            },
        )
        .unwrap();

        set_value(
            &store,
            key2,
            &v,
            CacheSetOptions {
                tags: Some(vec!["tag1".to_string()]),
                ttl: None,
                meta: None,
            },
        )
        .unwrap();

        store.purge_tag("tag2".to_string()).wait().unwrap();

        let conn = store.pool.get().unwrap();

        assert_eq!(
            false,
            redis::cmd("EXISTS")
                .arg(store.cache_key(key1))
                .query::<bool>(&*conn)
                .unwrap()
        );
    }

    #[test]
    fn test_redis_cache_set_w_ttl() {
        let store = setup();
        let mut v = [0u8; 1000];
        thread_rng().fill_bytes(&mut v);
        let key = "testsetwttl";

        set_value(
            &store,
            key,
            &v,
            CacheSetOptions {
                tags: None,
                ttl: Some(100),
                meta: None,
            },
        )
        .unwrap();

        let entry = store.get(key.to_string()).wait().unwrap().unwrap();
        assert_eq!(v.to_vec(), entry.stream.concat2().wait().unwrap());

        let conn = store.pool.get().unwrap();

        assert_eq!(
            redis::cmd("TTL")
                .arg(store.cache_key(key))
                .query::<i32>(&*conn)
                .unwrap(),
            100
        );
    }

    #[test]
    fn test_redis_cache_expire() {
        let store = setup();
        let key = "testexpire";

        store
            .set(
                key.to_string(),
                Box::new(stream::empty::<Vec<u8>, ()>()),
                CacheSetOptions {
                    ttl: None,
                    meta: None,
                    tags: None,
                },
            )
            .wait()
            .unwrap();

        store.expire(key.to_string(), 100).wait().unwrap();

        let res = store.ttl(key.to_string()).wait().unwrap();

        assert_eq!(res, 100);
    }

    #[test]
    fn test_redis_cache_del() {
        let store = setup();
        let key = "testdel";

        store
            .set(
                key.to_string(),
                Box::new(stream::empty::<Vec<u8>, ()>()),
                CacheSetOptions {
                    ttl: None,
                    meta: None,
                    tags: None,
                },
            )
            .wait()
            .unwrap();

        store.del(key.to_string()).wait().unwrap();

        assert!(store.get(key.to_string()).wait().unwrap().is_none());
    }

    #[test]
    fn test_redis_cache_set_tags() {
        let store = setup();
        let key = "testsettags";

        store
            .set(
                key.to_string(),
                Box::new(stream::empty::<Vec<u8>, ()>()),
                CacheSetOptions {
                    ttl: None,
                    meta: None,
                    tags: None,
                },
            )
            .wait()
            .unwrap();

        let tags = vec!["hello".to_string(), "world".to_string()];
        store
            .set_tags(key.to_string(), tags.clone())
            .wait()
            .unwrap();

        let conn = store.pool.get().unwrap();

        let fullkey = store.cache_key(key);
        let ts = redis::cmd("HGET")
            .arg(&fullkey)
            .arg("ts")
            .query::<i32>(&*conn)
            .unwrap();

        for tag in tags.iter() {
            assert_eq!(
                ts,
                redis::cmd("ZSCORE")
                    .arg(store.tag_key(tag))
                    .arg(&fullkey)
                    .query::<i32>(&*conn)
                    .unwrap()
            );
        }
    }
}
