use futures::{future, sync::oneshot, Future, Stream};
use std::net::SocketAddr;

use crate::runtime::{JsBody, JsHttpRequest, JsHttpResponse};
use crate::{RuntimeSelector, NEXT_EVENT_ID};

use std::sync::atomic::Ordering;

use hyper::body::Payload;
use hyper::{header, Body, Request, Response, StatusCode};

use std::sync::mpsc::RecvError;

pub fn serve_http(
    req: Request<Body>,
    selector: &RuntimeSelector,
    remote_addr: SocketAddr,
) -> Box<Future<Item = Response<Body>, Error = futures::Canceled> + Send> {
    let (parts, body) = req.into_parts();
    let host = {
        match parts.headers.get(header::HOST) {
            Some(v) => match v.to_str() {
                Ok(s) => s,
                Err(e) => {
                    error!("error stringifying host: {}", e);
                    return Box::new(future::ok(
                        Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::from("Bad host header"))
                            .unwrap(),
                    ));
                }
            },
            None => {
                return Box::new(future::ok(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap(),
                ))
            }
        }
    };

    let rt = match selector.get_by_hostname(host) {
        Ok(maybe_rt) => match maybe_rt {
            Some(rt) => rt,
            None => {
                return Box::new(future::ok(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("app not found"))
                        .unwrap(),
                ));
            }
        },
        Err(e) => {
            error!("error getting runtime: {:?}", e);
            return Box::new(future::ok(
                Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Body::empty())
                    .unwrap(),
            ));
        }
    };

    if rt.fetch_events.is_none() {
        return Box::new(future::ok(
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::empty())
                .unwrap(),
        ));
    }

    let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let url = format!("http://{}{}", host, parts.uri.path_and_query().unwrap());

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
            return Box::new(future::ok(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap(),
            ));
        }

        Box::new(rx.and_then(|res: JsHttpResponse| {
            let (mut parts, mut body) = Response::<Body>::default().into_parts();
            parts.headers = res.headers;
            parts.status = res.status;

            if let Some(js_body) = res.body {
                body = match js_body {
                    JsBody::Stream(s) => Body::wrap_stream(s.map_err(|_| RecvError {})),
                    JsBody::BytesStream(s) => {
                        Body::wrap_stream(s.map_err(|_| RecvError {}).map(|bm| bm.freeze()))
                    }
                    JsBody::Static(b) => Body::from(b),
                    _ => unimplemented!(),
                };
            }

            future::ok(Response::from_parts(parts, body))
        }))
    } else {
        Box::new(future::ok(
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::empty())
                .unwrap(),
        ))
    }
}
