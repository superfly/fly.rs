use futures::{future, Canceled, Future};
use http::header;
use hyper::{Body, Request, Response, StatusCode};

use prometheus::{Encoder, HistogramVec, IntCounterVec, IntGaugeVec, TextEncoder};

pub fn serve_metrics_http(
    _req: Request<Body>,
) -> Box<Future<Item = Response<Body>, Error = Canceled> + Send> {
    let metrics = prometheus::gather();
    let mut buf = vec![];
    if let Err(_) = ENCODER.encode(&metrics, &mut buf) {
        error!("unknown error encoding prometheus metrics!");
        return Box::new(future::ok(
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap(),
        ));
    }
    Box::new(future::ok(
        Response::builder()
            .header(header::CONTENT_TYPE, ENCODER.format_type())
            .body(Body::from(buf))
            .unwrap(),
    ))
}

lazy_static! {
    static ref ENCODER: TextEncoder = TextEncoder::new();
    pub static ref HTTP_RESPONSE_COUNTER: IntCounterVec = register_int_counter_vec!(
        "fly_http_responses_total",
        "Total number of HTTP responses made.",
        &["runtime", "version", "status"]
    )
    .unwrap();
    pub static ref HTTP_RESPONSE_TIME_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "fly_http_response_time_histogram_seconds",
        "HTTP response times by runtime, in seconds.",
        &["runtime", "version", "status"],
        vec![0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 1.0, 5.0, 10.0, 60.0]
    )
    .unwrap();
    pub static ref RUNTIME_USED_HEAP_GAUGE: IntGaugeVec = register_int_gauge_vec!(
        "fly_runtime_used_heap_size_bytes",
        "Used heap for a runtime, in bytes.",
        &["runtime", "version"]
    )
    .unwrap();
    pub static ref RUNTIME_TOTAL_HEAP_GAUGE: IntGaugeVec = register_int_gauge_vec!(
        "fly_runtime_total_heap_size_bytes",
        "Total heap for a runtime, in bytes.",
        &["runtime", "version"]
    )
    .unwrap();
    pub static ref RUNTIME_EXTERNAL_ALLOCATIONS_GAUGE: IntGaugeVec = register_int_gauge_vec!(
        "fly_runtime_externally_allocated_size_bytes",
        "Externally allocated size for a runtime, in bytes.",
        &["runtime", "version"]
    )
    .unwrap();
    pub static ref RUNTIME_MALLOCED_MEMORY_GAUGE: IntGaugeVec = register_int_gauge_vec!(
        "fly_runtime_malloced_memory_bytes",
        "Amount of memory, obtained via malloc, for a runtime, in bytes.",
        &["runtime", "version"]
    )
    .unwrap();
    pub static ref RUNTIME_PEAK_MALLOCED_MEMORY_GAUGE: IntGaugeVec = register_int_gauge_vec!(
        "fly_runtime_peak_malloced_memory_bytes",
        "Peak amount of memory, obtained via malloc, for a runtime, in bytes.",
        &["runtime", "version"]
    )
    .unwrap();
    pub static ref CACHE_GET_DURATION: HistogramVec = register_histogram_vec!(
        "fly_cache_get_duration_seconds",
        "Cache get duration in seconds.",
        &["type", "ns"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 1.0, 5.0, 10.0]
    )
    .unwrap();
    pub static ref CACHE_SET_DURATION: HistogramVec = register_histogram_vec!(
        "fly_cache_set_duration_seconds",
        "Cache set duration in seconds.",
        &["type", "ns"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 1.0, 5.0, 10.0]
    )
    .unwrap();
    pub static ref CACHE_HITS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_hits_total",
        "Cache hits totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_MISSES_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_misses_total",
        "Cache misses totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_errors_total",
        "Cache errors totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_GETS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_gets_total",
        "Cache gets totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_SETS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_sets_total",
        "Cache sets totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_DELS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_dels_total",
        "Cache dels totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_EXPIRES_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_expires_total",
        "Cache expires totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_TTLS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_ttls_total",
        "Cache ttls totals.",
        &["type", "ns"]
    )
    .unwrap();
    pub static ref CACHE_PURGES_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_purges_total",
        "Cache purges totals.",
        &["type", "ns", "tag"]
    )
    .unwrap();
    pub static ref CACHE_SET_TAGS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fly_cache_set_tags_total",
        "Cache set tags totals.",
        &["type", "ns"]
    )
    .unwrap();
}
