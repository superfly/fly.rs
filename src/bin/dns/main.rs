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
use trust_dns_proto::rr::{Record, RrsetRecords};
use trust_dns_server::authority::authority::LookupRecords;

use futures::sync::oneshot;

use std::io;
use trust_dns_server::server::{Request, RequestHandler, ResponseHandler, ServerFuture};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate flatbuffers;
use flatbuffers::FlatBufferBuilder;

#[macro_use]
extern crate log;
extern crate env_logger;
extern crate fly;
extern crate libfly;

use tokio::prelude::*;

use fly::msg;

use fly::ops::dns::*;
use fly::runtime::*;
use fly::utils::*;

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

  let runtime = {
    let rt = Runtime::new(None);
    rt.eval_file(matches.value_of("input").unwrap());
    rt
  };

  let handler = DnsHandler { runtime };

  let port: u16 = match matches.value_of("port") {
    Some(pstr) => pstr.parse::<u16>().unwrap(),
    None => 8053,
  };

  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
  let server = ServerFuture::new(handler);

  let udp_socket = UdpSocket::bind(&addr).expect(&format!("udp bind failed: {}", addr));
  info!("Listener bound on address: {}", addr);

  let main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

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

    let builder = &mut FlatBufferBuilder::new();

    let eid = fly::NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let queries: Vec<_> = req
      .message
      .queries()
      .iter()
      .map(|q| {
        debug!("query: {:?}", q);
        use self::dns::rr::{DNSClass, Name, RecordType};
        let name = builder.create_string(&Name::from(q.name().clone()).to_utf8());
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
      }).collect();

    let req_queries = builder.create_vector(&queries);

    let req_msg = msg::DnsRequest::create(
      builder,
      &msg::DnsRequestArgs {
        id: eid,
        message_type: msg::DnsMessageType::Query,
        queries: Some(req_queries),
        ..Default::default()
      },
    );

    let rt = &self.runtime;
    let rtptr = rt.ptr;

    let to_send = fly_buf_from(
      serialize_response(
        0,
        builder,
        msg::BaseArgs {
          msg: Some(req_msg.as_union_value()),
          msg_type: msg::Any::DnsRequest,
          ..Default::default()
        },
      ).unwrap(),
    );

    let (p, c) = oneshot::channel::<JsDnsResponse>();
    {
      rt.dns_responses.lock().unwrap().insert(eid, p);
    }

    {
      let rtptr = rtptr.clone();
      rt.event_loop
        .lock()
        .unwrap()
        .spawn(future::lazy(move || {
          unsafe { libfly::js_send(rtptr.0, to_send, null_buf()) };
          Ok(())
        })).unwrap();
    }

    let dns_res: JsDnsResponse = c.wait().unwrap();
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
