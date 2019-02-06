use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::Runtime;
use libfly::*;

use crate::js::*;
use crate::utils::*;

use hyper::Method;

use futures::{sync::mpsc, Future, Stream};

use trust_dns as dns;

pub fn op_add_event_ln(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let msg = base.msg_as_add_event_listener().unwrap();
    let ptr = rt.ptr;

    match msg.event() {
        msg::EventType::Fetch => {
            let (tx, rx) = mpsc::unbounded::<JsHttpRequest>();
            rt.spawn(
                rx.map_err(|_| error!("error event receiving http request"))
                    .for_each(move |req| {
                        let builder = &mut FlatBufferBuilder::new();

                        let req_url = builder.create_string(req.url.as_str());

                        let req_method = match req.method {
                            Method::GET => msg::HttpMethod::Get,
                            Method::POST => msg::HttpMethod::Post,
                            Method::PUT => msg::HttpMethod::Put,
                            Method::DELETE => msg::HttpMethod::Delete,
                            Method::HEAD => msg::HttpMethod::Head,
                            Method::OPTIONS => msg::HttpMethod::Options,
                            Method::CONNECT => msg::HttpMethod::Connect,
                            Method::PATCH => msg::HttpMethod::Patch,
                            Method::TRACE => msg::HttpMethod::Trace,
                            _ => unimplemented!(),
                        };

                        let headers: Vec<_> = req
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

                        let req_headers = builder.create_vector(&headers);
                        let req_remote_addr =
                            builder.create_string(req.remote_addr.ip().to_string().as_str());

                        let req_msg = msg::HttpRequest::create(
                            builder,
                            &msg::HttpRequestArgs {
                                id: req.id,
                                method: req_method,
                                url: Some(req_url),
                                headers: Some(req_headers),
                                remote_addr: Some(req_remote_addr),
                                has_body: req.body.is_some(),
                                ..Default::default()
                            },
                        );

                        let to_send = fly_buf_from(
                            serialize_response(
                                0,
                                builder,
                                msg::BaseArgs {
                                    msg: Some(req_msg.as_union_value()),
                                    msg_type: msg::Any::HttpRequest,
                                    ..Default::default()
                                },
                            )
                            .unwrap(),
                        );

                        ptr.send(to_send, None);

                        if let Some(stream) = req.body {
                            send_body_stream(ptr, req.id, stream);
                        }

                        Ok(())
                    })
                    .and_then(|_| Ok(debug!("done listening to http events."))),
            );
            rt.fetch_events = Some(tx);
        }
        msg::EventType::Resolv => {
            let (tx, rx) = mpsc::unbounded::<JsDnsRequest>();
            rt.spawn(
                rx.map_err(|_| error!("error event receiving http request"))
                    .for_each(move |req| {
                        let builder = &mut FlatBufferBuilder::new();

                        let queries: Vec<_> = req
                            .queries
                            .iter()
                            .map(|q| {
                                debug!("query: {:?}", q);
                                use self::dns::rr::{DNSClass, Name, RecordType};
                                let name =
                                    builder.create_string(&Name::from(q.name().clone()).to_utf8());
                                let rr_type = match q.query_type() {
                                    RecordType::A => msg::DnsRecordType::A,
                                    RecordType::AAAA => msg::DnsRecordType::AAAA,
                                    RecordType::AXFR => msg::DnsRecordType::AXFR,
                                    RecordType::CAA => msg::DnsRecordType::CAA,
                                    RecordType::CNAME => msg::DnsRecordType::CNAME,
                                    RecordType::IXFR => msg::DnsRecordType::IXFR,
                                    RecordType::MX => msg::DnsRecordType::MX,
                                    RecordType::NS => msg::DnsRecordType::NS,
                                    RecordType::NULL => msg::DnsRecordType::NULL,
                                    RecordType::OPT => msg::DnsRecordType::OPT,
                                    RecordType::PTR => msg::DnsRecordType::PTR,
                                    RecordType::SOA => msg::DnsRecordType::SOA,
                                    RecordType::SRV => msg::DnsRecordType::SRV,
                                    RecordType::TLSA => msg::DnsRecordType::TLSA,
                                    RecordType::TXT => msg::DnsRecordType::TXT,
                                    _ => unimplemented!(),
                                };
                                let dns_class = match q.query_class() {
                                    DNSClass::IN => msg::DnsClass::IN,
                                    DNSClass::CH => msg::DnsClass::CH,
                                    DNSClass::HS => msg::DnsClass::HS,
                                    DNSClass::NONE => msg::DnsClass::NONE,
                                    DNSClass::ANY => msg::DnsClass::ANY,
                                    _ => unimplemented!(),
                                };

                                msg::DnsQuery::create(
                                    builder,
                                    &msg::DnsQueryArgs {
                                        name: Some(name),
                                        rr_type: rr_type,
                                        dns_class: dns_class,
                                        ..Default::default()
                                    },
                                )
                            })
                            .collect();

                        let req_queries = builder.create_vector(&queries);

                        let req_msg = msg::DnsRequest::create(
                            builder,
                            &msg::DnsRequestArgs {
                                id: req.id,
                                message_type: match req.message_type {
                                    dns::op::MessageType::Query => msg::DnsMessageType::Query,
                                    _ => unimplemented!(),
                                },
                                queries: Some(req_queries),
                                ..Default::default()
                            },
                        );

                        let to_send = fly_buf_from(
                            serialize_response(
                                0,
                                builder,
                                msg::BaseArgs {
                                    msg: Some(req_msg.as_union_value()),
                                    msg_type: msg::Any::DnsRequest,
                                    ..Default::default()
                                },
                            )
                            .unwrap(),
                        );

                        ptr.send(to_send, None);
                        Ok(())
                    }),
            );
            rt.resolv_events = Some(tx);
        }
    };

    ok_future(None)
}
