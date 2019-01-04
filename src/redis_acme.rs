use crate::acme_store::*;
use crate::settings::RedisStoreConfig;
use futures::{future, Future};
use redis::Commands;
use std::fmt::Display;
extern crate r2d2_redis;
use self::r2d2_redis::RedisConnectionManager;
use self::r2d2_redis::{r2d2, redis};

pub struct RedisAcmeStore {
    pool: r2d2::Pool<RedisConnectionManager>,
    ns: Option<String>,
}

impl RedisAcmeStore {
    pub fn new(conf: &RedisStoreConfig) -> Self {
        RedisAcmeStore {
            pool: r2d2::Pool::new(RedisConnectionManager::new(conf.url.as_str()).unwrap()).unwrap(),
            ns: conf.namespace.as_ref().cloned(),
        }
    }

    fn challenge_key<S: Display>(&self, hostname: S, token: S) -> String {
        match &self.ns {
            Some(ns) => format!("{}:acme-challenge:{}:{}", &ns, hostname, token),
            None => format!("acme-challenge:{}:{}", hostname, token),
        }
    }
}

impl AcmeStore for RedisAcmeStore {
    fn get_challenge(
        &self,
        hostname: String,
        token: String,
    ) -> Box<Future<Item = Option<String>, Error = AcmeError> + Send> {
        let fullkey = self.challenge_key(hostname, token);
        debug!("redis acme get with key: {}", fullkey);

        let pool = self.pool.clone();
        Box::new(future::lazy(move || match pool.get() {
            Err(e) => Err(AcmeError::Failure(format!("{}", e))),
            Ok(conn) => match conn.get(&fullkey) {
                Err(e) => Err(AcmeError::Failure(format!("{}", e))),
                Ok(validation) => Ok(validation),
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(ns: Option<String>) -> RedisAcmeStore {
        RedisAcmeStore::new(&RedisStoreConfig {
            url: "redis://localhost:6379".to_string(),
            namespace: ns,
        })
    }

    fn set_token(store: &RedisAcmeStore, hostname: &str, token: &str, signature: &str) {
        let conn = store.pool.get().unwrap();
        let key = store.challenge_key(&hostname, &token);

        let _: () = conn
            .set(key.to_string(), format!("{}.{}", token, signature))
            .expect("Failed to set token");
    }

    #[test]
    fn test_get_challenge() {
        let store = setup(Some("test".to_string()));

        set_token(&store, "fly.io", "valid-token", "valid-signature");

        let cases = [
            (
                "fly.io",
                "valid-token",
                Some("valid-token.valid-signature".to_owned()),
            ),
            ("fly.io", "missing-token.valid-signature", None),
            ("edgeapp.net", "valid-token.missing-signature", None),
        ];

        for test in cases.iter() {
            let actual = store
                .get_challenge(test.0.to_string(), test.1.to_string())
                .wait()
                .unwrap();
            assert_eq!(actual, test.2);
        }
    }

    #[test]
    fn test_key_with_namespace() {
        let store = setup(Some("with-namespace".to_string()));
        assert_eq!(
            "with-namespace:acme-challenge:hostname:token",
            store.challenge_key("hostname", "token")
        );
    }

    #[test]
    fn test_key_without_namespace() {
        let store = setup(None);
        assert_eq!(
            "acme-challenge:hostname:token",
            store.challenge_key("hostname", "token")
        );
    }
}
