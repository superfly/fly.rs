extern crate config;

use std::sync::RwLock;

lazy_static! {
    pub static ref GLOBAL_SETTINGS: RwLock<GlobalSettings> = {
        let mut settings = config::Config::new();
        settings.merge(config::Environment::new()).unwrap();
        RwLock::new(settings.try_into().unwrap())
    };
}

#[derive(Debug, Deserialize)]
pub struct GlobalSettings {
    // pub host: String,
    pub cluster_url: String,
    pub node_ip: String,
    pub region: String,
    // pub proxy_env: String,
    pub log_level: String,
    // pub backhaul_token: String,
    pub redis_url: String,
    pub redis_cache_url: String,
    // pub redis_cache_notifier_url: String,
    // pub redis_cache_notifier_writer_url: String,
    // pub bugsnag_api_key: String,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_region: Option<String>,
    // pub aws_sqs_queue_url: String,
    // pub fly_private_api_host: String,
    // pub fly_private_api_token: String,
    pub blacklist_ip_path: Option<String>,
    // pub geoip_path: String,
    // pub cert_path: String,
    pub cockroach_certs_path: Option<String>,
    pub cockroach_host: String,
    pub proxy_port: Option<u16>,
    pub proxy_tls_port: Option<u16>,
    pub proxy_bind_ip: Option<String>,
    // pub proxy_dns_port: Option<u16>,
    pub prometheus_host: Option<String>,
    pub prometheus_port: Option<String>,
    // pub uv_threadpool_size: String,
    // pub logger_host: String,
    // pub logger_port: String,
}

#[derive(Debug, Deserialize)]
pub enum ProxyPort {
    Port(i16),
    Socket(String),
}
