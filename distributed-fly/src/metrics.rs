use prometheus::Histogram;

lazy_static! {
    pub static ref TLS_HANDSHAKE_TIME_HISTOGRAM: Histogram = register_histogram!(
        "fly_tls_handshake_time_histogram_seconds",
        "TLS handshake times by runtime, in seconds.",
        vec![0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 1.0, 5.0, 10.0, 60.0, 120.0]
    )
    .unwrap();
}
