use futures::Future;

use fly::{runtime::Runtime, RuntimeSelector, SelectorError};

use std::collections::HashMap;
use std::sync::RwLock;

use release::Release;
use settings::GLOBAL_SETTINGS;

pub struct DistributedRuntimeSelector {
    pub runtimes: RwLock<HashMap<String, Box<Runtime>>>,
}

impl DistributedRuntimeSelector {
    pub fn new() -> Self {
        DistributedRuntimeSelector {
            runtimes: RwLock::new(HashMap::new()),
        }
    }
}

impl RuntimeSelector for DistributedRuntimeSelector {
    fn get_by_hostname(&self, hostname: &str) -> Result<Option<&mut Runtime>, SelectorError> {
        let rel = match Release::get(hostname) {
            Err(e) => return Err(SelectorError::Failure(e)),
            Ok(maybe_rel) => match maybe_rel {
                None => return Ok(None),
                Some(rel) => rel,
            },
        };

        let key = format!("{}:{}", rel.app_id, rel.version);

        let runtimes = &self.runtimes;

        {
            if !runtimes.read().unwrap().contains_key(&key) {
                let settings = {
                    use fly::settings::*;
                    let global_settings = &*GLOBAL_SETTINGS.read().unwrap();
                    Settings {
                        data_store: Some(DataStore::Postgres(PostgresStoreConfig {
                            url: global_settings.cockroach_host.clone(),
                            database: Some(format!("objectstore_{}", rel.app_id)),
                            tls_ca_crt: if let Some(ref certs_path) =
                                global_settings.cockroach_certs_path
                            {
                                Some(format!("{}/ca.crt", certs_path))
                            } else {
                                None
                            },
                            tls_client_crt: if let Some(ref certs_path) =
                                global_settings.cockroach_certs_path
                            {
                                Some(format!("{}/client.root.crt", certs_path))
                            } else {
                                None
                            },
                            tls_client_key: if let Some(ref certs_path) =
                                global_settings.cockroach_certs_path
                            {
                                Some(format!("{}/client.root.key", certs_path))
                            } else {
                                None
                            },
                        })), // TODO: use postgres store
                        cache_store: Some(CacheStore::Redis(RedisStoreConfig {
                            url: global_settings.redis_cache_url.clone(),
                            namespace: Some(rel.app_id.to_string()),
                        })), // TODO: use redis store
                        fs_store: Some(FsStore::Redis(RedisStoreConfig {
                            namespace: Some(format!("app:{}:release:latest:file:", rel.app_id)),
                            url: global_settings.redis_url.clone(),
                        })),
                    }
                };
                let mut rt = Runtime::new(Some(rel.app.clone()), &settings);
                let merged_conf = rel.clone().parsed_config().unwrap();
                rt.eval(
                    "<app config>",
                    &format!("window.app = {{ config: {} }};", merged_conf),
                );
                rt.eval("app.js", &rel.source);
                let app = rel.app;
                let app_id = rel.app_id;
                let version = rel.version;

                // TODO: ughh, refactor!
                // let _key2 = key.clone();
                tokio::spawn(rt.run().then(move |res: Result<(), _>| {
                    if let Err(_) = res {
                        error!("app: {} ({}) v{} ended abruptly", app, app_id, version);
                    }
                    // runtimes.write().unwrap().remove(&key2);
                    Ok(())
                }));
                {
                    debug!("writing runtime in hashmap");
                    runtimes.write().unwrap().insert(key.clone(), rt);
                }
            }
        }

        let runtimes = runtimes.read().unwrap(); // TODO: no unwrap
        match runtimes.get(&key) {
            Some(rt) => Ok(Some(rt.ptr.to_runtime())),
            None => Ok(None),
        }
    }
}
