use futures::{
    future,
    sync::{mpsc, oneshot},
};

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::js::*;
use crate::runtime::{Runtime, EVENT_LOOP};
use crate::utils::*;
use libfly::*;

use crate::errors::{FlyError, FlyResult};

use crate::get_next_stream_id;

use hyper::body::Payload;
use hyper::client::HttpConnector;
use hyper::header::HeaderName;
use hyper::rt::{Future, Stream};
use hyper::HeaderMap;
use hyper::{Body, Client, Method, Request, StatusCode};

use hyper_tls::HttpsConnector;

use std::io;

use std::slice;

use crate::metrics::*;
use floating_duration::TimeAsFloat;
use http::uri::Scheme;
use std::time;

lazy_static! {
    static ref HTTP_CLIENT: Client<HttpsConnector<HttpConnector>, Body> = {
        Client::builder()
            .executor(EVENT_LOOP.0.clone())
            .build(HttpsConnector::new(4).unwrap())
    };
}

pub fn op_fetch(rt: &mut Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_http_request().unwrap();

    let url = msg.url().unwrap();
    if url.starts_with("file://") {
        return file_request(rt, cmd_id, url);
    }

    let ptr = rt.ptr;

    let req_id = msg.id();

    let http_uri: hyper::Uri = match url.parse() {
        Ok(u) => u,
        Err(e) => return odd_future(format!("{}", e).into()),
    };

    // for the metrics
    let host_str = http_uri.host().unwrap_or("unknown");
    let host = if let Some(port) = http_uri.port_part() {
        format!("{}:{}", host_str, port.as_str())
    } else {
        let port = if let Some(scheme) = http_uri.scheme_part() {
            if scheme == &Scheme::HTTPS {
                "443"
            } else {
                "80"
            }
        } else {
            "80"
        };
        format!("{}:{}", host_str, port)
    };

    FETCH_HTTP_REQUESTS_TOTAL
        .with_label_values(&[rt.name.as_str(), rt.version.as_str(), host.as_str()])
        .inc();

    let method = match msg.method() {
        msg::HttpMethod::Get => Method::GET,
        msg::HttpMethod::Head => Method::HEAD,
        msg::HttpMethod::Post => Method::POST,
        msg::HttpMethod::Put => Method::PUT,
        msg::HttpMethod::Patch => Method::PATCH,
        msg::HttpMethod::Delete => Method::DELETE,
        msg::HttpMethod::Connect => Method::CONNECT,
        msg::HttpMethod::Options => Method::OPTIONS,
        msg::HttpMethod::Trace => Method::TRACE,
    };

    let msg_headers = msg.headers().unwrap();
    let mut headers = HeaderMap::new();
    for i in 0..msg_headers.len() {
        let h = msg_headers.get(i);
        trace!("header: {} => {}", h.key().unwrap(), h.value().unwrap());
        headers.insert(
            HeaderName::from_bytes(h.key().unwrap().as_bytes()).unwrap(),
            h.value().unwrap().parse().unwrap(),
        );
    }

    let has_body = msg.has_body();
    trace!("HAS BODY? {}", has_body);
    let req_body = if has_body {
        if raw.data_len > 0 {
            trace!("STATIC BODY!");
            Body::from(unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec())
        } else {
            trace!("STREAMING BODY");
            let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
            {
                rt.streams.lock().unwrap().insert(req_id, sender);
            }
            Body::wrap_stream(recver.map_err(|_| std::sync::mpsc::RecvError {}))
        }
    } else {
        Body::empty()
    };
    // let req_body = Body::empty();

    let mut req = Request::new(req_body);
    {
        *req.uri_mut() = http_uri.clone();
        *req.method_mut() = method;
        *req.headers_mut() = headers;
    }

    let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

    let rt_name = rt.name.clone();
    let rt_version = rt.version.clone();
    let method = req.method().clone();

    rt.spawn(future::lazy(move || {
        let timer = time::Instant::now();
        HTTP_CLIENT.request(req).then(move |reserr| {
            debug!("got http response (or error)");
            if let Err(err) = reserr {
                if p.send(Err(err.into())).is_err() {
                    error!("error sending error for http response :/");
                }
                return Ok(());
            }

            let res = reserr.unwrap(); // should be safe.

            FETCH_HEADERS_DURATION
                .with_label_values(&[
                    rt_name.as_str(),
                    rt_version.as_str(),
                    method.as_str(),
                    host.as_str(),
                    res.status().as_str(),
                ])
                .observe(timer.elapsed().as_fractional_secs());

            let (parts, body) = res.into_parts();

            let mut stream_rx: Option<JsBody> = None;
            let has_body = !body.is_end_stream();
            if has_body {
                stream_rx = Some(JsBody::BoxedStream(Box::new(
                    body.map_err(|e| format!("{}", e).into()).map(move |chunk| {
                        let bytes = chunk.into_bytes();
                        DATA_IN_TOTAL
                            .with_label_values(&[rt_name.as_str(), rt_version.as_str(), "fetch"])
                            .inc_by(bytes.len() as i64);
                        bytes.to_vec()
                    }),
                )));
            }

            if p.send(Ok(JsHttpResponse {
                headers: parts.headers,
                status: parts.status,
                body: stream_rx,
            }))
            .is_err()
            {
                error!("error sending fetch http response");
                return Ok(());
            }
            debug!("done with http request");
            Ok(())
        })
    }));

    let fut = c
        .map_err(|e| {
            FlyError::from(io::Error::new(
                io::ErrorKind::Other,
                format!("err getting response from oneshot: {}", e).as_str(),
            ))
        })
        .and_then(move |reserr: FlyResult<JsHttpResponse>| {
            if let Err(err) = reserr {
                return Err(err);
            }

            let res = reserr.unwrap();

            let builder = &mut FlatBufferBuilder::new();
            let headers: Vec<_> = res
                .headers
                .iter()
                .map(|(key, value)| {
                    let key = builder.create_string(key.as_str());
                    let value = builder.create_string(value.to_str().unwrap());
                    msg::HttpHeader::create(
                        builder,
                        &msg::HttpHeaderArgs {
                            key: Some(key),
                            value: Some(value),
                            ..Default::default()
                        },
                    )
                })
                .collect();

            let res_headers = builder.create_vector(&headers);

            let msg = msg::FetchHttpResponse::create(
                builder,
                &msg::FetchHttpResponseArgs {
                    id: req_id,
                    headers: Some(res_headers),
                    status: res.status.as_u16(),
                    has_body: res.body.is_some(),
                    ..Default::default()
                },
            );
            if let Some(stream) = res.body {
                send_body_stream(ptr, req_id, stream);
            }
            Ok(serialize_response(
                cmd_id,
                builder,
                msg::BaseArgs {
                    msg: Some(msg.as_union_value()),
                    msg_type: msg::Any::FetchHttpResponse,
                    ..Default::default()
                },
            ))
        });

    Box::new(fut)
}

pub fn op_http_response(rt: &mut Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
    debug!("handling http response");
    let msg = base.msg_as_http_response().unwrap();
    let req_id = msg.id();

    let status = match StatusCode::from_u16(msg.status()) {
        Ok(s) => s,
        Err(e) => return odd_future(format!("{}", e).into()),
    };

    let mut headers = HeaderMap::new();

    if let Some(msg_headers) = msg.headers() {
        for i in 0..msg_headers.len() {
            let h = msg_headers.get(i);
            headers.insert(
                HeaderName::from_bytes(h.key().unwrap().as_bytes()).unwrap(),
                h.value().unwrap().parse().unwrap(),
            );
        }
    }

    let mut body: Option<JsBody> = None;
    let has_body = msg.has_body();
    if has_body {
        if raw.data_len == 0 {
            debug!("http response will have a streaming body");
            let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
            {
                let mut streams = rt.streams.lock().unwrap();
                streams.insert(req_id, sender);
            }
            body = Some(JsBody::Stream(recver));
        } else {
            debug!("http response will have a static body");
            body = Some(JsBody::Static(
                unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec(),
            ));
        }
    }

    let mut responses = rt.responses.lock().unwrap();
    match responses.remove(&req_id) {
        Some(sender) => {
            if sender
                .send(JsHttpResponse {
                    headers: headers,
                    status: status,
                    body: body,
                })
                .is_err()
            {
                return odd_future("error sending http response".to_string().into());
            }
        }
        None => return odd_future("no response receiver!".to_string().into()),
    };

    ok_future(None)
}

fn file_request(rt: &mut Runtime, cmd_id: u32, url: &str) -> Box<Op> {
    let req_id = get_next_stream_id();
    let path: String = url.chars().skip(7).collect();

    let ptr = rt.ptr;

    Box::new(
        rt.fs_store
            .read(path)
            .map_err(|e| format!("fs error: {:?}", e).into())
            .and_then(move |maybe_entry| {
                let builder = &mut FlatBufferBuilder::new();

                let msg = msg::FetchHttpResponse::create(
                    builder,
                    &msg::FetchHttpResponseArgs {
                        id: req_id,
                        headers: None,
                        status: if maybe_entry.is_some() { 404 } else { 200 },
                        has_body: maybe_entry.is_some(),
                        ..Default::default()
                    },
                );
                if let Some(entry) = maybe_entry {
                    send_body_stream(
                        ptr,
                        req_id,
                        JsBody::BoxedStream(Box::new(
                            entry.stream.map_err(|e| format!("{:?}", e).into()),
                        )),
                    );
                }
                Ok(serialize_response(
                    cmd_id,
                    builder,
                    msg::BaseArgs {
                        msg: Some(msg.as_union_value()),
                        msg_type: msg::Any::FetchHttpResponse,
                        ..Default::default()
                    },
                ))
            }),
    )
}
