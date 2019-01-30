use futures::{future, Future, Stream};
use std::net::SocketAddr;

use crate::js::*;
use crate::metrics::*;
use crate::utils::*;
use crate::{get_next_stream_id, RuntimeSelector};

use hyper::body::Payload;
use hyper::{header, Body, Request, Response, StatusCode};

use floating_duration::TimeAsFloat;
use std::io;
use std::time;

type BoxedResponseFuture = Box<Future<Item = Response<Body>, Error = futures::Canceled> + Send>;

lazy_static! {
    // static ref SERVER_HEADER: &'static str =
    static ref SERVER_HEADER_VALUE: header::HeaderValue = {
        let s = format!("Fly ({})", crate::BUILD_VERSION);
        header::HeaderValue::from_str(s.as_str()).unwrap()
    };
}

pub fn serve_http(
    tls: bool,
    req: Request<Body>,
    selector: &RuntimeSelector,
    remote_addr: SocketAddr,
) -> BoxedResponseFuture {
    let timer = time::Instant::now();
    debug!("serving http: {}", req.uri());
    let req_id = ksuid::Ksuid::generate().to_base62();
    let (parts, body) = req.into_parts();
    let host = if parts.version == hyper::Version::HTTP_2 {
        match parts.uri.host() {
            Some(h) => h,
            None => {
                return future_response(
                    simple_response(req_id, StatusCode::NOT_FOUND, None),
                    timer,
                    None,
                );
            }
        }
    } else {
        match parts.headers.get(header::HOST) {
            Some(v) => match v.to_str() {
                Ok(s) => s,
                Err(e) => {
                    error!("error stringifying host: {}", e);
                    return future_response(
                        simple_response(
                            req_id,
                            StatusCode::BAD_REQUEST,
                            Some(Body::from("Bad host header")),
                        ),
                        timer,
                        None,
                    );
                }
            },
            None => {
                return future_response(
                    simple_response(req_id, StatusCode::NOT_FOUND, None),
                    timer,
                    None,
                );
            }
        }
    };

    let rt = match selector.get_by_hostname(host) {
        Ok(maybe_rt) => match maybe_rt {
            Some(rt) => rt,
            None => {
                return future_response(
                    simple_response(
                        req_id,
                        StatusCode::NOT_FOUND,
                        Some(Body::from("app not found")),
                    ),
                    timer,
                    None,
                );
            }
        },
        Err(e) => {
            error!("error getting runtime: {:?}", e);
            return future_response(
                simple_response(req_id, StatusCode::SERVICE_UNAVAILABLE, None),
                timer,
                None,
            );
        }
    };

    let url: String = if parts.version == hyper::Version::HTTP_2 {
        format!("{}", parts.uri)
    } else {
        format!(
            "{}://{}{}",
            if tls { "https" } else { "http" },
            host,
            parts.uri.path_and_query().unwrap()
        )
    };
    let stream_id = get_next_stream_id();

    let rt_name = rt.name.clone();
    let rt_version = rt.version.clone();

    match rt.dispatch_event(
        stream_id,
        JsEvent::Fetch(JsHttpRequest {
            id: stream_id,
            method: parts.method,
            remote_addr: remote_addr,
            url: url,
            headers: parts.headers.clone(),
            body: if body.is_end_stream() {
                None
            } else {
                Some(JsBody::BoxedStream(Box::new(
                    body.map_err(|e| format!("{}", e).into()).map(move |chunk| {
                        let bytes = chunk.into_bytes();
                        DATA_IN_TOTAL
                            .with_label_values(&[
                                rt_name.as_str(),
                                rt_version.as_str(),
                                "http_request",
                            ])
                            .inc_by(bytes.len() as i64);
                        bytes.to_vec()
                    }),
                )))
            },
        }),
    ) {
        None => future_response(
            simple_response(req_id, StatusCode::SERVICE_UNAVAILABLE, None),
            timer,
            Some((rt.name.clone(), rt.version.clone())),
        ),
        Some(Err(e)) => {
            error!("error sending js http request: {:?}", e);
            future_response(
                simple_response(req_id, StatusCode::INTERNAL_SERVER_ERROR, None),
                timer,
                Some((rt.name.clone(), rt.version.clone())),
            )
        }
        Some(Ok(EventResponseChannel::Http(rx))) => {
            let rt_name = rt.name.clone();
            let rt_version = rt.version.clone();
            wrap_future(
                rx.and_then(move |res: JsHttpResponse| {
                    let (mut parts, mut body) = Response::<Body>::default().into_parts();
                    parts.headers = res.headers;
                    parts.status = res.status;

                    if let Some(js_body) = res.body {
                        body = match js_body {
                            JsBody::Stream(s) => Body::wrap_stream(
                                s.map_err(|_| {
                                    io::Error::new(io::ErrorKind::Interrupted, "interrupted stream")
                                })
                                .inspect(move |v| {
                                    DATA_OUT_TOTAL
                                        .with_label_values(&[
                                            rt_name.as_str(),
                                            rt_version.as_str(),
                                            "http_response",
                                        ])
                                        .inc_by(v.len() as i64);
                                }),
                            ),
                            JsBody::BytesStream(s) => Body::wrap_stream(
                                s.map_err(|_| {
                                    io::Error::new(io::ErrorKind::Interrupted, "interrupted stream")
                                })
                                .map(|bm| bm.freeze())
                                .inspect(move |bytes| {
                                    DATA_OUT_TOTAL
                                        .with_label_values(&[
                                            rt_name.as_str(),
                                            rt_version.as_str(),
                                            "http_response",
                                        ])
                                        .inc_by(bytes.len() as i64);
                                }),
                            ),
                            JsBody::Static(b) => {
                                DATA_OUT_TOTAL
                                    .with_label_values(&[
                                        rt_name.as_str(),
                                        rt_version.as_str(),
                                        "http_response",
                                    ])
                                    .inc_by(b.len() as i64);
                                Body::from(b)
                            }
                            _ => unimplemented!(),
                        };
                    }

                    Ok(set_request_id(
                        set_server_header(Response::from_parts(parts, body)),
                        req_id,
                    ))
                }),
                timer,
                Some((rt.name.clone(), rt.version.clone())),
            )
        }
        _ => unimplemented!(),
    }
}

fn future_response(
    res: Response<Body>,
    timer: time::Instant,
    namever: Option<(String, String)>,
) -> BoxedResponseFuture {
    wrap_future(future::ok(res), timer, namever)
}

fn simple_response(req_id: String, status: StatusCode, body: Option<Body>) -> Response<Body> {
    set_request_id(
        set_server_header(
            Response::builder()
                .status(status)
                .body(body.unwrap_or(Body::empty()))
                .unwrap(),
        ),
        req_id,
    )
}

fn set_server_header(mut res: Response<Body>) -> Response<Body> {
    res.headers_mut()
        .insert(header::SERVER, SERVER_HEADER_VALUE.clone());
    res
}

fn set_request_id(mut res: Response<Body>, req_id: String) -> Response<Body> {
    res.headers_mut().insert(
        "fly-request-id",
        header::HeaderValue::from_str(req_id.as_str()).unwrap(),
    );
    res
}

fn wrap_future<F>(
    fut: F,
    timer: time::Instant,
    namever: Option<(String, String)>,
) -> BoxedResponseFuture
where
    F: Future<Item = Response<Body>, Error = futures::Canceled> + Send + 'static,
{
    Box::new(fut.and_then(move |res| {
        let (name, ver) = namever.unwrap_or((String::new(), String::new()));
        let status = res.status();
        let status_str = status.as_str();
        HTTP_RESPONSE_TIME_HISTOGRAM
            .with_label_values(&[name.as_str(), ver.as_str(), status_str])
            .observe(timer.elapsed().as_fractional_secs());
        HTTP_RESPONSE_COUNTER
            .with_label_values(&[name.as_str(), ver.as_str(), status_str])
            .inc();
        Ok(res)
    }))
}

// static APPLICATION_X_JAVASCRIPT: &str = "application/x-javascript";
// static APPLICATION_VND_MS_FONTOBJECT: &str = "application/vnd.ms-fontobject";
// static APPLICATION_X_FONT_OPENTYPE: &str = "application/x-font-opentype";
// static APPLICATION_X_FONT_TRUETYPE: &str = "application/x-font-truetype";
// static APPLICATION_X_FONT_TTF: &str = "application/x-font-ttf";
// static FONT_EOT: &str = "font/eot";
// static FONT_OPENTYPE: &str = "font/opentype";
// static FONT_OTF: &str = "font/otf";
// static IMAGE_VND_MICROSOFT_ICON: &str = "image/vnd.microsoft.icon";

// static OTHER_ALLOWED_MIME_TYPES: [&str; 9] = [
//     APPLICATION_X_JAVASCRIPT,
//     APPLICATION_VND_MS_FONTOBJECT,
//     APPLICATION_X_FONT_OPENTYPE,
//     APPLICATION_X_FONT_TRUETYPE,
//     APPLICATION_X_FONT_TTF,
//     FONT_EOT,
//     FONT_OPENTYPE,
//     FONT_OTF,
//     IMAGE_VND_MICROSOFT_ICON,
// ];

// fn gunzip(chunk: hyper::Chunk) -> Result<Vec<u8>, FlyError> {
//     let bytes = chunk.into_bytes();
//     let mut v = vec![];
//     let mut gz = GzDecoder::new(&bytes[..]);
//     match gz.read_to_end(&mut v) {
//         Ok(_) => Ok(v),
//         Err(e) => Err(format!("gzip decode error: {}", e).into()),
//     }
// }

// fn gzip<B>(bytes: B) -> Result<Vec<u8>, io::Error>
// where
//     B: AsRef<[u8]>,
// {
//     let mut v = vec![];
//     let mut gz = GzEncoder::new(bytes.as_ref(), Compression::default());
//     gz.read_to_end(&mut v)?;
//     Ok(v)
// }

// fn contains_gzip(header_value: Option<&header::HeaderValue>) -> bool {
//     if let Some(enc) = header_value {
//         if let Ok(encstr) = enc.to_str() {
//             if encstr.contains("gzip") {
//                 true
//             } else {
//                 false
//             }
//         } else {
//             false
//         }
//     } else {
//         false
//     }
// }

// fn gzippable_content_type(header_value: Option<&header::HeaderValue>) -> bool {
//     if let Some(maybe_content_type) = header_value {
//         if let Ok(content_type) = maybe_content_type.to_str() {
//             if let Ok(m) = mime::Mime::from_str(content_type) {
//                 if m.type_() == mime::TEXT {
//                     true
//                 } else if m.type_() == mime::APPLICATION {
//                     match m.subtype() {
//                         mime::JAVASCRIPT | mime::JSON | mime::XML => true,
//                         _ => false,
//                     }
//                 } else if m.type_() == mime::IMAGE && m.subtype() == mime::SVG {
//                     true
//                 } else {
//                     false
//                 }
//             } else if OTHER_ALLOWED_MIME_TYPES.contains(&content_type) {
//                 true
//             } else {
//                 false
//             }
//         } else {
//             false
//         }
//     } else {
//         false
//     }
// }
