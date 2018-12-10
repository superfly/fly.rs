use futures::sync::{mpsc, oneshot};

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::{JsBody, JsHttpResponse, JsRuntime, Op, EVENT_LOOP};
use crate::utils::*;
use libfly::*;

use crate::errors::{FlyError, FlyResult};

use crate::NEXT_EVENT_ID;

use std::sync::atomic::Ordering;

extern crate hyper;

use self::hyper::body::Payload;
use self::hyper::client::HttpConnector;
use self::hyper::header::HeaderName;
use self::hyper::rt::{Future, Stream};
use self::hyper::HeaderMap;
use self::hyper::{Body, Client, Method, Request, StatusCode};

extern crate hyper_tls;
use self::hyper_tls::HttpsConnector;

use std::io;

use std::slice;

lazy_static! {
    static ref HTTP_CLIENT: Client<HttpsConnector<HttpConnector>, Body> = {
        Client::builder()
            .executor(EVENT_LOOP.0.clone())
            .build(HttpsConnector::new(4).unwrap())
    };
}

pub fn op_fetch(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_http_request().unwrap();

    let url = msg.url().unwrap();
    if url.starts_with("file://") {
        return file_request(ptr, cmd_id, url);
    }

    let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let req_body: Body;
    if msg.has_body() {
        warn!("has body not implemented!");
        unimplemented!(); // TODO: implement
    } else {
        req_body = Body::empty();
    }

    let mut req = Request::new(req_body);
    {
        let uri: hyper::Uri = url.parse().unwrap();
        *req.uri_mut() = uri;
        *req.method_mut() = match msg.method() {
            msg::HttpMethod::Get => Method::GET,
            msg::HttpMethod::Head => Method::HEAD,
            msg::HttpMethod::Post => Method::POST,
            _ => {
                warn!("method not implemented");
                unimplemented!()
            }
        };

        let msg_headers = msg.headers().unwrap();
        let headers = req.headers_mut();
        for i in 0..msg_headers.len() {
            let h = msg_headers.get(i);
            trace!("header: {} => {}", h.key().unwrap(), h.value().unwrap());
            headers.insert(
                HeaderName::from_bytes(h.key().unwrap().as_bytes()).unwrap(),
                h.value().unwrap().parse().unwrap(),
            );
        }
    }

    let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

    let rt = ptr.to_runtime();

    rt.spawn(HTTP_CLIENT.request(req).then(move |reserr| {
        debug!("got http response (or error)");
        if let Err(err) = reserr {
            if p.send(Err(err.into())).is_err() {
                error!("error sending error for http response :/");
            }
            return Ok(());
        }

        let res = reserr.unwrap(); // should be safe.

        let (parts, body) = res.into_parts();

        let mut stream_rx: Option<JsBody> = None;
        let has_body = !body.is_end_stream();
        if has_body {
            stream_rx = Some(JsBody::HyperBody(body));
        }

        if p.send(Ok(JsHttpResponse {
            headers: parts.headers,
            status: parts.status,
            body: stream_rx,
        }))
        .is_err()
        {
            error!("error sending http response");
            return Ok(());
        }
        debug!("done with http request");
        Ok(())
    }));

    let fut = c
        .map_err(|e| {
            FlyError::from(io::Error::new(
                io::ErrorKind::Other,
                format!("err getting response from oneshot: {}", e).as_str(),
            ))
        })
        .and_then(move |reserr: FlyResult<JsHttpResponse>| {
            debug!("IN HTTP RESPONSE RECEIVING END");
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

pub fn op_http_response(ptr: JsRuntime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
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

    let rt = ptr.to_runtime();

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

fn file_request(ptr: JsRuntime, cmd_id: u32, url: &str) -> Box<Op> {
    let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;
    let path: String = url.chars().skip(7).collect();

    let rt = ptr.to_runtime();

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

// use tokio::codec::{BytesCodec, FramedRead};
//
// fn op_file_request(ptr: JsRuntime, cmd_id: u32, url: &str) -> Box<Op> {
//   let req_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

//   let (p, c) = oneshot::channel::<FlyResult<JsHttpResponse>>();

//   let path: String = url.chars().skip(7).collect();

//   let meta = match fs::metadata(path.clone()) {
//     Ok(m) => m,
//     Err(e) => return odd_future(e.into()),
//   };

//   println!("META: {:?}", meta);

//   if meta.is_file() {
//     EVENT_LOOP.0.spawn(future::lazy(move || {
//       tokio::fs::File::open(path).then(
//         move |fileerr: Result<tokio::fs::File, io::Error>| -> Result<(), ()> {
//           debug!("file opened? {}", fileerr.is_ok());
//           if let Err(err) = fileerr {
//             if p.send(Err(err.into())).is_err() {
//               error!("error sending file open error");
//             }
//             return Ok(());
//           }

//           let file = fileerr.unwrap(); // should be safe.

//           let (tx, rx) = mpsc::unbounded::<BytesMut>();

//           if p
//             .send(Ok(JsHttpResponse {
//               headers: HeaderMap::new(),
//               status: StatusCode::OK,
//               body: Some(JsBody::BytesStream(rx)),
//             })).is_err()
//           {
//             error!("error sending http response");
//             return Ok(());
//           }

//           EVENT_LOOP.0.spawn(
//             FramedRead::new(file, BytesCodec::new())
//               .map_err(|e| println!("error reading file chunk! {}", e))
//               .for_each(move |chunk| {
//                 debug!("got frame chunk");
//                 match tx.clone().unbounded_send(chunk) {
//                   Ok(_) => Ok(()),
//                   Err(e) => {
//                     error!("error sending chunk in channel: {}", e);
//                     Err(())
//                   }
//                 }
//               }),
//           );
//           Ok(())
//         },
//       )
//     }));
//   } else {
//     EVENT_LOOP.0.spawn(future::lazy(move || {
//       tokio::fs::read_dir(path).then(move |read_dir_err| {
//         if let Err(err) = read_dir_err {
//           if p.send(Err(err.into())).is_err() {
//             error!("error sending read_dir error");
//           }
//           return Ok(());
//         }
//         let read_dir = read_dir_err.unwrap();
//         let (tx, rx) = mpsc::unbounded::<Vec<u8>>();

//         if p
//           .send(Ok(JsHttpResponse {
//             headers: HeaderMap::new(),
//             status: StatusCode::OK,
//             body: Some(JsBody::Stream(rx)),
//           })).is_err()
//         {
//           error!("error sending http response");
//           return Ok(());
//         }

//         EVENT_LOOP.0.spawn(
//           read_dir
//             .map_err(|e| println!("error read_dir stream: {}", e))
//             .for_each(move |entry| {
//               let entrypath = entry.path();
//               let pathstr = format!("{}\n", entrypath.to_str().unwrap());
//               match tx.clone().unbounded_send(pathstr.into()) {
//                 Ok(_) => Ok(()),
//                 Err(e) => {
//                   error!("error sending path chunk in channel: {}", e);
//                   Err(())
//                 }
//               }
//             }),
//         );
//         Ok(())
//       })
//     }));
//   }

//   let fut = c
//     .map_err(|e| {
//       FlyError::from(io::Error::new(
//         io::ErrorKind::Other,
//         format!("err getting response from oneshot: {}", e).as_str(),
//       ))
//     }).and_then(move |reserr: FlyResult<JsHttpResponse>| {
//       if let Err(err) = reserr {
//         return Err(err);
//       }

//       let res = reserr.unwrap();

//       let builder = &mut FlatBufferBuilder::new();
//       let headers: Vec<_> = res
//         .headers
//         .iter()
//         .map(|(key, value)| {
//           let key = builder.create_string(key.as_str());
//           let value = builder.create_string(value.to_str().unwrap());
//           msg::HttpHeader::create(
//             builder,
//             &msg::HttpHeaderArgs {
//               key: Some(key),
//               value: Some(value),
//               ..Default::default()
//             },
//           )
//         }).collect();

//       let res_headers = builder.create_vector(&headers);

//       let msg = msg::FetchHttpResponse::create(
//         builder,
//         &msg::FetchHttpResponseArgs {
//           id: req_id,
//           headers: Some(res_headers),
//           status: res.status.as_u16(),
//           has_body: res.body.is_some(),
//           ..Default::default()
//         },
//       );
//       if let Some(stream) = res.body {
//         send_body_stream(ptr, req_id, stream);
//       }
//       Ok(serialize_response(
//         cmd_id,
//         builder,
//         msg::BaseArgs {
//           msg: Some(msg.as_union_value()),
//           msg_type: msg::Any::FetchHttpResponse,
//           ..Default::default()
//         },
//       ))
//     });

//   Box::new(fut)
// }
