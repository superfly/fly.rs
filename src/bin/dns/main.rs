extern crate futures;
use futures::future;

extern crate tokio;
extern crate tokio_udp;

use tokio::prelude::*;

use tokio_udp::UdpSocket;

extern crate trust_dns;
extern crate trust_dns_server;

use trust_dns::op;

use std::io;
use trust_dns_server::server::{Request, RequestHandler, ResponseHandler, ServerFuture};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn main() {
  let handler = DnsHandler {};
  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8053);
  let mut server = ServerFuture::new(handler);

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
    println!("dns(req): {}: {:?}", req.src, req.message);

    let mut msg = op::Message::new();
    msg.set_id(req.message.id());

    res.send(msg)
  }
}
