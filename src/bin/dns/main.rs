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

// use dns::rr::{LowerName, Name};

use futures::sync::oneshot;
// use std::sync::mpsc::RecvError;

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
extern crate toml;

use tokio::prelude::*;
use tokio::timer::Interval;

use std::time::Duration;

use std::fs::File;

use fly::config::Config;

use fly::msg;

use fly::runtime::*;

use env_logger::Env;

#[macro_use]
extern crate lazy_static;
extern crate num_cpus;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::sync::RwLock;

lazy_static! {
  static ref NCPUS: usize = num_cpus::get();
  static ref REQ_PER_APP: RwLock<HashMap<String, AtomicUsize>> = RwLock::new(HashMap::new());
}

fn main() {
  let env = Env::default().filter_or("LOG_LEVEL", "info");

  let handler = DnsHandler {};
  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8053);
  let server = ServerFuture::new(handler);

  let udp_socket = UdpSocket::bind(&addr).expect(&format!("udp bind failed: {}", addr));
  info!("listening for udp on {:?}", udp_socket);

  info!("V8 version: {}", libfly::version());

  env_logger::init_from_env(env);

  let mut main_el = tokio::runtime::Runtime::new().unwrap();
  unsafe {
    EVENT_LOOP_HANDLE = Some(main_el.executor());
  };

  let mut file = File::open("fly.toml").unwrap();
  let mut contents = String::new();
  file.read_to_string(&mut contents).unwrap();
  let conf: Config = toml::from_str(&contents).unwrap();

  debug!("toml: {:?}", conf);

  for (name, app) in conf.apps.unwrap().iter() {
    {
      let mut rts = RUNTIMES.write().unwrap();
      let mut rtsv: Vec<Box<Runtime>> = vec![];
      let filename = app.filename.as_str();
      for _i in 0..*NCPUS {
        let rt = Runtime::new(Some(name.to_string()));
        rt.eval_file(filename);
        rtsv.push(rt);
      }
      rts.insert(name.to_string(), rtsv);
      REQ_PER_APP
        .write()
        .unwrap()
        .insert(name.to_string(), ATOMIC_USIZE_INIT);
    };
  }

  let task = Interval::new_interval(Duration::from_secs(5))
    .for_each(move |_| {
        match RUNTIMES.read() {
            Ok(_rts) => {
                // for (key, rt) in rts.iter() {
                //     let stats = rt.heap_statistics();
                //     info!(
                //         "[heap stats for {0}] used: {1:.2}MB | total: {2:.2}MB | alloc: {3:.2}MB | malloc: {4:.2}MB | peak malloc: {5:.2}MB",
                //         key,
                //         stats.used_heap_size as f64 / (1024_f64 * 1024_f64),
                //         stats.total_heap_size as f64 / (1024_f64 * 1024_f64),
                //         stats.externally_allocated as f64 / (1024_f64 * 1024_f64),
                //         stats.malloced_memory as f64 / (1024_f64 * 1024_f64),
                //         stats.peak_malloced_memory as f64 / (1024_f64 * 1024_f64),
                //     );
                // }
            }
            Err(e) => error!("error locking runtimes: {}", e),
        };
        Ok(())
    }).map_err(|e| panic!("interval errored; err={:?}", e));

  main_el.spawn(task);
  let _ = main_el.block_on_all(future::lazy(move || -> Result<(), ()> {
    server.register_socket(udp_socket);
    Ok(())
  }));

  // main_el.shutdown_on_idle();
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

    let builder = &mut FlatBufferBuilder::new();

    let eid = fly::NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let queries: Vec<_> = req
      .message
      .queries()
      .iter()
      .map(|q| {
        println!("query: {:?}", q);
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

    let guard = RUNTIMES.read().unwrap();
    let rtsv = guard.values().next().unwrap();

    let idx = {
      let map = REQ_PER_APP.read().unwrap();
      let counter = map.values().next().unwrap();
      counter.fetch_add(1, Ordering::Relaxed) % rtsv.len()
    };

    let rt = &rtsv[idx];
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
      rt.rt
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
