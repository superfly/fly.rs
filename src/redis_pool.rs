use r2d2_redis::r2d2;
use r2d2_redis::RedisConnectionManager;

use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    static ref REDIS_POOLS: Mutex<HashMap<String, r2d2::Pool<RedisConnectionManager>>> =
        Mutex::new(HashMap::new());
}

pub fn get_pool(url: String) -> r2d2::Pool<RedisConnectionManager> {
    REDIS_POOLS
        .lock()
        .unwrap()
        .entry(url.clone())
        .or_insert_with(move || {
            r2d2::Pool::builder()
                .max_size(100)
                .min_idle(Some(0))
                .build(RedisConnectionManager::new(url.as_str()).unwrap())
                .unwrap() // should not fail because min_idle set to 0
        })
        .clone() // that's like Arc::clone, no worries.
}
