use futures::{future, sync::oneshot, Future, Stream};
use std::net::SocketAddr;

use crate::metrics;
use crate::runtime::{JsBody, JsHttpRequest, JsHttpResponse};
use crate::{RuntimeSelector, NEXT_EVENT_ID};

use std::sync::atomic::Ordering;

use hyper::body::Payload;
use hyper::{header, Body, Request, Response, StatusCode};

use floating_duration::TimeAsFloat;
use std::io;
use std::time;

type BoxedResponseFuture = Box<Future<Item = Response<Body>, Error = futures::Canceled> + Send>;

pub fn serve_http(
    tls: bool,
    req: Request<Body>,
    selector: &RuntimeSelector,
    remote_addr: SocketAddr,
) -> BoxedResponseFuture {
    let timer = time::Instant::now();
    info!("serving http: {}", req.uri());
    let (parts, body) = req.into_parts();
    warn!("headers: {:?}", parts.headers);
    let host = if parts.version == hyper::Version::HTTP_2 {
        match parts.uri.host() {
            Some(h) => h,
            None => {
                return future_response(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap(),
                    timer,
                    None,
                )
            }
        }
    } else {
        match parts.headers.get(header::HOST) {
            Some(v) => match v.to_str() {
                Ok(s) => s,
                Err(e) => {
                    error!("error stringifying host: {}", e);
                    return future_response(
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::from("Bad host header"))
                            .unwrap(),
                        timer,
                        None,
                    );
                }
            },
            None => {
                return future_response(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap(),
                    timer,
                    None,
                )
            }
        }
    };

    let rt = match selector.get_by_hostname(host) {
        Ok(maybe_rt) => match maybe_rt {
            Some(rt) => rt,
            None => {
                return future_response(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("app not found"))
                        .unwrap(),
                    timer,
                    None,
                );
            }
        },
        Err(e) => {
            error!("error getting runtime: {:?}", e);
            return future_response(
                Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Body::empty())
                    .unwrap(),
                timer,
                None,
            );
        }
    };

    if rt.fetch_events.is_none() {
        return future_response(
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::empty())
                .unwrap(),
            timer,
            Some((rt.name.clone(), rt.version.clone())),
        );
    }

    let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

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

    // double checking, could've been removed since last check (maybe not? may need a lock)
    if let Some(ref ch) = rt.fetch_events {
        let rx = {
            let (tx, rx) = oneshot::channel::<JsHttpResponse>();
            rt.responses.lock().unwrap().insert(req_id, tx);
            rx
        };
        let sendres = ch.unbounded_send(JsHttpRequest {
            id: req_id,
            method: parts.method,
            remote_addr: remote_addr,
            url: url,
            headers: parts.headers.clone(),
            body: if body.is_end_stream() {
                None
            } else {
                Some(JsBody::HyperBody(body))
            },
        });

        if let Err(e) = sendres {
            error!("error sending js http request: {}", e);
            return future_response(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap(),
                timer,
                Some((rt.name.clone(), rt.version.clone())),
            );
        }

        wrap_future(
            rx.and_then(move |res: JsHttpResponse| {
                let (mut parts, mut body) = Response::<Body>::default().into_parts();
                parts.headers = res.headers;
                parts.status = res.status;

                if let Some(js_body) = res.body {
                    body = match js_body {
                        JsBody::Stream(s) => Body::wrap_stream(s.map_err(|_| {
                            io::Error::new(io::ErrorKind::Interrupted, "interrupted stream")
                        })),
                        JsBody::BytesStream(s) => Body::wrap_stream(
                            s.map_err(|_| {
                                io::Error::new(io::ErrorKind::Interrupted, "interrupted stream")
                            })
                            .map(|bm| bm.freeze()),
                        ),
                        JsBody::Static(b) => Body::from(b),
                        _ => unimplemented!(),
                    };
                }

                Ok(Response::from_parts(parts, body))
            }),
            timer,
            Some((rt.name.clone(), rt.version.clone())),
        )
    } else {
        future_response(
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::empty())
                .unwrap(),
            timer,
            Some((rt.name.clone(), rt.version.clone())),
        )
    }
}

fn future_response(
    res: Response<Body>,
    timer: time::Instant,
    namever: Option<(String, String)>,
) -> BoxedResponseFuture {
    wrap_future(future::ok(res), timer, namever)
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
        metrics::HTTP_RESPONSE_TIME_HISTOGRAM
            .with_label_values(&[name.as_str(), ver.as_str(), status_str])
            .observe(timer.elapsed().as_fractional_secs());
        metrics::HTTP_RESPONSE_COUNTER
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
