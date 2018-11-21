extern crate futures;
use futures::future;

extern crate tokio;
extern crate tokio_udp;

use tokio_udp::UdpSocket;

extern crate trust_dns as dns;
extern crate trust_dns_proto;
extern crate trust_dns_server;

use trust_dns_server::authority::{AuthLookup, MessageResponseBuilder};

use trust_dns_proto::op::header::Header;
use trust_dns_proto::op::response_code::ResponseCode;
use trust_dns_proto::rr::{Record, RrsetRecords};
use trust_dns_server::authority::authority::LookupRecords;

use futures::sync::oneshot;

use std::io;
use trust_dns_server::server::{Request, RequestHandler, ResponseHandler, ServerFuture};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate flatbuffers;

#[macro_use]
extern crate log;
extern crate env_logger;
extern crate fly;
extern crate libfly;

use tokio::prelude::*;

use fly::ops::dns::*;
use fly::runtime::*;
use fly::settings::SETTINGS;

use env_logger::Env;

use std::sync::atomic::Ordering;

extern crate clap;

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "info");
  env_logger::init_from_env(env);
  debug!("V8 version: {}", libfly::version());

  let matches = clap::App::new("fly-dns")
    .version("0.0.1-alpha")
    .about("Fly DNS server")
    .arg(
      clap::Arg::with_name("port")
        .short("p")
        .long("port")
        .takes_value(true),
    ).arg(
      clap::Arg::with_name("input")
        .help("Sets the input file to use")
        .required(true)
        .index(1),
    ).get_matches();

  let main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  let entry_file = matches.value_of("input").unwrap();
  let mut runtime = Runtime::new(None, &SETTINGS.read().unwrap());

  debug!("Loading dev tools");
  runtime.eval_file("v8env/dist/dev-tools.js").unwrap();
  runtime
    .eval("<installDevTools>", "installDevTools();")
    .unwrap();
  debug!("Loading dev tools done");
  runtime
    .main_eval(entry_file, &format!("dev.run('{}')", entry_file))
    .unwrap();

  let handler = DnsHandler { runtime };

  let port: u16 = match matches.value_of("port") {
    Some(pstr) => pstr.parse::<u16>().unwrap(),
    None => 8053,
  };

  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
  let server = ServerFuture::new(handler);

  let udp_socket = UdpSocket::bind(&addr).expect(&format!("udp bind failed: {}", addr));
  info!("Listener bound on address: {}", addr);

  let _ = main_el.block_on_all(future::lazy(move || -> Result<(), ()> {
    server.register_socket(udp_socket);
    Ok(())
  }));
}

pub struct DnsHandler {
  runtime: Box<Runtime>,
}

impl RequestHandler for DnsHandler {
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

    let eid = fly::NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let rt = &self.runtime;

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
