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

use futures::sync::oneshot;

use self::trust_dns_server::server::{Request, RequestHandler, ResponseHandler, ServerFuture};
use std::io;

use std::net::SocketAddr;

extern crate flatbuffers;

use tokio::prelude::*;

use ops::dns::*;
use {RuntimeSelector, NEXT_EVENT_ID};

use std::sync::atomic::Ordering;

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

        let eid = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

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

        if rt.resolv_events.is_none() {
            return res.send_response(
                MessageResponseBuilder::new(Some(req.message.raw_queries())).error_msg(
                    req.message.id(),
                    req.message.op_code(),
                    ResponseCode::ServFail,
                ),
            );
        }

        let ch = rt.resolv_events.as_ref().unwrap();
        let rx = {
            let (tx, rx) = oneshot::channel::<JsDnsResponse>();
            rt.dns_responses.lock().unwrap().insert(eid, tx);
            rx
        };
        let sendres = ch.unbounded_send(JsDnsRequest {
            id: eid,
            message_type: req.message.message_type(),
            queries: req.message.queries().to_vec(),
        });
        if let Err(_e) = sendres {
            return res.send_response(
                MessageResponseBuilder::new(Some(req.message.raw_queries())).error_msg(
                    req.message.id(),
                    req.message.op_code(),
                    ResponseCode::ServFail,
                ),
            );
        }

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
            }).collect();
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
