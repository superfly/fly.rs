extern crate futures;
use futures::future;

extern crate tokio;
extern crate tokio_udp;

use tokio_udp::UdpSocket;

extern crate trust_dns;
extern crate trust_dns_server;

use trust_dns::op::{Message, MessageType, OpCode, Query, ResponseCode};
use trust_dns::rr::{RData, Record, RecordType};

use std::io;
use trust_dns_server::server::{Request, RequestHandler, ResponseHandler, ServerFuture};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn main() {
  let handler = DnsHandler {};
  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8053);
  let server = ServerFuture::new(handler);

  let udp_socket = UdpSocket::bind(&addr).expect(&format!("udp bind failed: {}", addr));
  println!("listening for udp on {:?}", udp_socket);

  tokio::run(future::lazy(move || {
    server.register_socket(udp_socket);
    Ok(())
  }))
}

pub struct DnsHandler;

impl RequestHandler for DnsHandler {
  fn handle_request<'q, 'a, R: ResponseHandler + 'static>(
    &'a self,
    req: &'q Request,
    res: R,
  ) -> io::Result<()> {
    println!(
      "dns(req): {:?} {}: {:?}",
      req.message.message_type(),
      req.src,
      req.message
    );

    // let mut queries: Vec<&Query> = vec![];

    let mut msg = Message::new();

    for q in req.message.queries() {
      let mut r = Record::new();

      r.set_name(q.name().clone().into())
        .set_rr_type(q.query_type())
        .set_rdata(RData::A([127, 0, 0, 1].into()))
        .set_dns_class(q.query_class())
        .set_ttl(10);

      // msg.add_query(q.original().clone());
      msg.add_answer(r);
    }

    msg
      .set_id(req.message.id())
      .set_op_code(OpCode::Query)
      .set_message_type(MessageType::Response)
      .set_response_code(ResponseCode::NoError)
      .set_authoritative(true);

    res.send(msg)
  }
}
