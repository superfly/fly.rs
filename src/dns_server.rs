extern crate tokio_udp;

use self::tokio_udp::UdpSocket;

extern crate trust_dns as dns;
extern crate trust_dns_proto;
extern crate trust_dns_server;

use self::trust_dns_server::authority::{AuthLookup, MessageResponseBuilder};

use self::trust_dns_proto::op::header::Header;
use self::trust_dns_proto::op::response_code::ResponseCode;
use self::trust_dns_proto::rr::{Record, RrsetRecords};
use self::trust_dns_server::authority::authority::LookupRecords;

use self::trust_dns_server::server::{Request, RequestHandler, ResponseHandler, ServerFuture};
use std::io;

use std::net::SocketAddr;

extern crate flatbuffers;

use tokio::prelude::*;

use crate::ops::dns::*;
use crate::runtime::{EventResponseChannel, JsEvent};
use crate::{get_next_stream_id, RuntimeSelector};

pub struct DnsServer {
    addr: SocketAddr,
    selector: &'static (RuntimeSelector + Send + Sync),
}

impl DnsServer {
    pub fn new(addr: SocketAddr, selector: &'static (RuntimeSelector + Send + Sync)) -> Self {
        DnsServer { addr, selector }
    }
    pub fn start(self) {
        let udp_socket =
            UdpSocket::bind(&self.addr).expect(&format!("udp bind failed: {}", self.addr));
        info!("Listener bound on address: {}", self.addr);
        let server = ServerFuture::new(self);
        server.register_socket(udp_socket);
    }
}

impl RequestHandler for DnsServer {
    fn handle_request<'q, 'a, R: ResponseHandler + 'static>(
        &'a self,
        req: &'q Request,
        res: R,
    ) -> io::Result<()> {
        debug!(
            "dns(req): {:?} {}: {:?}",
            req.message.message_type(),
            req.src,
            req.message
        );

        let eid = get_next_stream_id();

        let queries = req.message.queries();
        let mut name = dns::rr::Name::from(queries[0].name().clone())
            .trim_to(2)
            .to_utf8();
        name.pop();

        let rt = match self.selector.get_by_hostname(name.as_str()) {
            Ok(maybe_rt) => match maybe_rt {
                Some(rt) => rt,
                None => {
                    return res.send_response(
                        MessageResponseBuilder::new(Some(req.message.raw_queries())).error_msg(
                            req.message.id(),
                            req.message.op_code(),
                            ResponseCode::ServFail,
                        ),
                    )
                }
            },
            Err(e) => {
                error!("error getting runtime: {:?}", e);
                return res.send_response(
                    MessageResponseBuilder::new(Some(req.message.raw_queries())).error_msg(
                        req.message.id(),
                        req.message.op_code(),
                        ResponseCode::ServFail,
                    ),
                );
            }
        };

        let rx = match rt.dispatch_event(
            eid,
            JsEvent::Resolv(JsDnsRequest {
                id: eid,
                message_type: req.message.message_type(),
                queries: req.message.queries().to_vec(),
            }),
        ) {
            None => {
                return res.send_response(
                    MessageResponseBuilder::new(Some(req.message.raw_queries())).error_msg(
                        req.message.id(),
                        req.message.op_code(),
                        ResponseCode::ServFail,
                    ),
                )
            }
            Some(Err(e)) => {
                error!("error sending js dns request: {:?}", e);
                return res.send_response(
                    MessageResponseBuilder::new(Some(req.message.raw_queries())).error_msg(
                        req.message.id(),
                        req.message.op_code(),
                        ResponseCode::ServFail,
                    ),
                );
            }
            Some(Ok(EventResponseChannel::Dns(rx))) => rx,
            _ => unimplemented!(),
        };

        let dns_res: JsDnsResponse = rx.wait().unwrap();
        let answers: Vec<Record> = dns_res
            .answers
            .iter()
            .map(|ans| {
                Record::from_rdata(
                    ans.name.clone(),
                    ans.ttl,
                    ans.rdata.to_record_type(),
                    ans.rdata.to_owned(),
                )
            })
            .collect();
        let mut msg = MessageResponseBuilder::new(Some(req.message.raw_queries()));
        let msg = {
            msg.answers(AuthLookup::Records(LookupRecords::RecordsIter(
                RrsetRecords::RecordsOnly(answers.iter()),
            )));

            let mut header = Header::new();

            header
                .set_id(req.message.id())
                .set_op_code(dns_res.op_code)
                .set_message_type(dns_res.message_type)
                .set_response_code(dns_res.response_code)
                .set_authoritative(dns_res.authoritative)
                .set_truncated(dns_res.truncated);

            msg.build(header)
        };
        res.send_response(msg)
    }
}
