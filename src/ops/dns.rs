use flatbuffers::FlatBufferBuilder;
use msg;

extern crate trust_dns as dns;
extern crate trust_dns_proto as dns_proto;
use self::dns::client::ClientHandle; // necessary for trait to be in scope

use std::sync::Mutex;

use libfly::*;
use runtime::{JsRuntime, Op, EVENT_LOOP_HANDLE};
use utils::*;

use futures::Future;

lazy_static! {
  static ref DNS_RESOLVER: Mutex<dns::client::BasicClientHandle<dns_proto::xfer::DnsMultiplexerSerialResponse>> = {
    let (stream, handle) = dns::udp::UdpClientStream::new(([8, 8, 8, 8], 53).into());
    let (bg, client) = dns::client::ClientFuture::new(stream, handle, None);
    unsafe { EVENT_LOOP_HANDLE.as_ref().unwrap().spawn(bg) };
    Mutex::new(client)
  };
}

#[derive(Debug)]
pub struct JsDnsResponse {
  pub op_code: dns::op::OpCode,
  pub message_type: dns::op::MessageType,
  pub response_code: dns::op::ResponseCode,
  pub answers: Vec<JsDnsRecord>,
  pub queries: Vec<JsDnsQuery>,
  pub authoritative: bool,
  pub truncated: bool,
}

#[derive(Debug)]
pub struct JsDnsRequest {
  pub id: u32,
  pub message_type: dns::op::MessageType,
  pub queries: Vec<dns::op::LowerQuery>,
}

#[derive(Debug)]
pub struct JsDnsRecord {
  pub name: dns::rr::Name,
  pub rdata: dns::rr::RData,
  pub dns_class: dns::rr::DNSClass,
  pub ttl: u32,
}

#[derive(Debug)]
pub struct JsDnsQuery {
  pub name: dns::rr::Name,
  pub rr_type: dns::rr::RecordType,
  pub dns_class: dns::rr::DNSClass,
}

pub fn op_dns_query(_ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  println!("handle dns");
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_dns_query().unwrap();

  let query_type = match msg.rr_type() {
    msg::DnsRecordType::A => dns::rr::RecordType::A,
    msg::DnsRecordType::AAAA => dns::rr::RecordType::AAAA,
    msg::DnsRecordType::ANY => dns::rr::RecordType::ANY,
    msg::DnsRecordType::AXFR => dns::rr::RecordType::AXFR,
    msg::DnsRecordType::CAA => dns::rr::RecordType::CAA,
    msg::DnsRecordType::CNAME => dns::rr::RecordType::CNAME,
    msg::DnsRecordType::IXFR => dns::rr::RecordType::IXFR,
    msg::DnsRecordType::MX => dns::rr::RecordType::MX,
    msg::DnsRecordType::NS => dns::rr::RecordType::NS,
    msg::DnsRecordType::NULL => dns::rr::RecordType::NULL,
    msg::DnsRecordType::OPT => dns::rr::RecordType::OPT,
    msg::DnsRecordType::PTR => dns::rr::RecordType::PTR,
    msg::DnsRecordType::SOA => dns::rr::RecordType::SOA,
    msg::DnsRecordType::SRV => dns::rr::RecordType::SRV,
    msg::DnsRecordType::TLSA => dns::rr::RecordType::TLSA,
    msg::DnsRecordType::TXT => dns::rr::RecordType::TXT,
  };

  Box::new(
    DNS_RESOLVER
      .lock()
      .unwrap()
      .query(
        msg.name().unwrap().parse().unwrap(),
        dns::rr::DNSClass::IN,
        query_type,
      ).map_err(|e| format!("dns query error: {}", e).into())
      .and_then(move |res| {
        // println!("got a dns response! {:?}", res);
        for q in res.queries() {
          println!("queried: {:?}", q);
        }
        let builder = &mut FlatBufferBuilder::new();
        let answers: Vec<_> = res
          .answers()
          .iter()
          .map(|ans| {
            println!("answer: {:?}", ans);
            use self::dns::rr::{DNSClass, RData, RecordType};
            let name = builder.create_string(&ans.name().to_utf8());
            let rr_type = match ans.rr_type() {
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
            let dns_class = match ans.dns_class() {
              DNSClass::IN => msg::DnsClass::IN,
              DNSClass::CH => msg::DnsClass::CH,
              DNSClass::HS => msg::DnsClass::HS,
              DNSClass::NONE => msg::DnsClass::NONE,
              DNSClass::ANY => msg::DnsClass::ANY,
              _ => unimplemented!(),
            };
            let rdata_type = match ans.rdata() {
              RData::A(_) => msg::DnsRecordData::DnsA,
              RData::AAAA(_) => msg::DnsRecordData::DnsAaaa,
              RData::CNAME(_) => msg::DnsRecordData::DnsCname,
              RData::MX(_) => msg::DnsRecordData::DnsMx,
              RData::NS(_) => msg::DnsRecordData::DnsNs,
              RData::PTR(_) => msg::DnsRecordData::DnsPtr,
              RData::SOA(_) => msg::DnsRecordData::DnsSoa,
              RData::SRV(_) => msg::DnsRecordData::DnsSrv,
              RData::TXT(_) => msg::DnsRecordData::DnsTxt,
              _ => unimplemented!(),
            };
            let rdata = match ans.rdata() {
              RData::A(ip) => {
                let ipstr = builder.create_string(&ip.to_string());
                msg::DnsA::create(
                  builder,
                  &msg::DnsAArgs {
                    ip: Some(ipstr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::AAAA(ip) => {
                let ipstr = builder.create_string(&ip.to_string());
                msg::DnsAaaa::create(
                  builder,
                  &msg::DnsAaaaArgs {
                    ip: Some(ipstr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::CNAME(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsCname::create(
                  builder,
                  &msg::DnsCnameArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::MX(mx) => {
                let exstr = builder.create_string(&mx.exchange().to_utf8());
                msg::DnsMx::create(
                  builder,
                  &msg::DnsMxArgs {
                    exchange: Some(exstr),
                    preference: mx.preference(),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::NS(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsNs::create(
                  builder,
                  &msg::DnsNsArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::PTR(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsPtr::create(
                  builder,
                  &msg::DnsPtrArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::SOA(soa) => {
                let mnamestr = builder.create_string(&soa.mname().to_utf8());
                let rnamestr = builder.create_string(&soa.rname().to_utf8());
                msg::DnsSoa::create(
                  builder,
                  &msg::DnsSoaArgs {
                    mname: Some(mnamestr),
                    rname: Some(rnamestr),
                    serial: soa.serial(),
                    refresh: soa.refresh(),
                    retry: soa.retry(),
                    expire: soa.expire(),
                    minimum: soa.minimum(),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::SRV(srv) => {
                let targetstr = builder.create_string(&srv.target().to_utf8());
                msg::DnsSrv::create(
                  builder,
                  &msg::DnsSrvArgs {
                    priority: srv.priority(),
                    weight: srv.weight(),
                    port: srv.port(),
                    target: Some(targetstr),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              RData::TXT(txt) => {
                let coll: Vec<_> = txt
                  .iter()
                  .map(|t| {
                    let d = builder.create_vector(&Vec::from(t.clone()));
                    msg::DnsTxtData::create(
                      builder,
                      &msg::DnsTxtDataArgs {
                        data: Some(d),
                        ..Default::default()
                      },
                    )
                  }).collect();
                let data = builder.create_vector(&coll);

                msg::DnsTxt::create(
                  builder,
                  &msg::DnsTxtArgs {
                    data: Some(data),
                    ..Default::default()
                  },
                ).as_union_value()
              }
              _ => unimplemented!(),
            };

            msg::DnsRecord::create(
              builder,
              &msg::DnsRecordArgs {
                name: Some(name),
                rr_type: rr_type,
                dns_class: dns_class,
                ttl: ans.ttl(),
                rdata_type: rdata_type,
                rdata: Some(rdata),
                ..Default::default()
              },
            )
          }).collect();

        let res_answers = builder.create_vector(&answers);
        let dns_msg = msg::DnsResponse::create(
          builder,
          &msg::DnsResponseArgs {
            op_code: msg::DnsOpCode::Query,
            message_type: msg::DnsMessageType::Response,
            authoritative: res.authoritative(),
            truncated: res.truncated(),
            // response_code: ,
            answers: Some(res_answers),
            // done: body.is_end_stream(),
            ..Default::default()
          },
        );

        Ok(serialize_response(
          cmd_id,
          builder,
          msg::BaseArgs {
            msg: Some(dns_msg.as_union_value()),
            msg_type: msg::Any::DnsResponse,
            ..Default::default()
          },
        ))
      }),
  )
}

pub fn op_dns_response(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_dns_response().unwrap();
  let req_id = msg.id();

  let op_code = match msg.op_code() {
    msg::DnsOpCode::Query => dns::op::OpCode::Query,
    msg::DnsOpCode::Status => dns::op::OpCode::Status,
    msg::DnsOpCode::Notify => dns::op::OpCode::Notify,
    msg::DnsOpCode::Update => dns::op::OpCode::Update,
  };

  let res_code = msg.response_code() as u16;

  let message_type = match msg.message_type() {
    msg::DnsMessageType::Query => dns::op::MessageType::Query,
    msg::DnsMessageType::Response => dns::op::MessageType::Response,
  };

  use self::dns::rr::RData;

  let queries: Vec<JsDnsQuery> = if let Some(msg_queries) = msg.queries() {
    let qlen = msg_queries.len();
    let mut queries: Vec<JsDnsQuery> = Vec::with_capacity(qlen);
    for i in 0..qlen {
      let q = msg_queries.get(i);

      let rr_type = match q.rr_type() {
        msg::DnsRecordType::A => dns::rr::RecordType::A,
        msg::DnsRecordType::AAAA => dns::rr::RecordType::AAAA,
        msg::DnsRecordType::ANY => dns::rr::RecordType::ANY,
        msg::DnsRecordType::AXFR => dns::rr::RecordType::AXFR,
        msg::DnsRecordType::CAA => dns::rr::RecordType::CAA,
        msg::DnsRecordType::CNAME => dns::rr::RecordType::CNAME,
        msg::DnsRecordType::IXFR => dns::rr::RecordType::IXFR,
        msg::DnsRecordType::MX => dns::rr::RecordType::MX,
        msg::DnsRecordType::NS => dns::rr::RecordType::NS,
        msg::DnsRecordType::NULL => dns::rr::RecordType::NULL,
        msg::DnsRecordType::OPT => dns::rr::RecordType::OPT,
        msg::DnsRecordType::PTR => dns::rr::RecordType::PTR,
        msg::DnsRecordType::SOA => dns::rr::RecordType::SOA,
        msg::DnsRecordType::SRV => dns::rr::RecordType::SRV,
        msg::DnsRecordType::TLSA => dns::rr::RecordType::TLSA,
        msg::DnsRecordType::TXT => dns::rr::RecordType::TXT,
      };

      let dns_class = match q.dns_class() {
        msg::DnsClass::IN => dns::rr::DNSClass::IN,
        msg::DnsClass::CH => dns::rr::DNSClass::CH,
        msg::DnsClass::HS => dns::rr::DNSClass::HS,
        msg::DnsClass::NONE => dns::rr::DNSClass::NONE,
        msg::DnsClass::ANY => dns::rr::DNSClass::ANY,
      };

      queries.push(JsDnsQuery {
        name: q.name().unwrap().parse().unwrap(),
        rr_type: rr_type,
        dns_class: dns_class,
      });
    }
    vec![]
  } else {
    vec![]
  };

  let answers = if let Some(msg_answers) = msg.answers() {
    let anslen = msg_answers.len();
    let mut answers: Vec<JsDnsRecord> = Vec::with_capacity(anslen);
    for i in 0..anslen {
      let ans = msg_answers.get(i);

      let dns_class = match ans.dns_class() {
        msg::DnsClass::IN => dns::rr::DNSClass::IN,
        msg::DnsClass::CH => dns::rr::DNSClass::CH,
        msg::DnsClass::HS => dns::rr::DNSClass::HS,
        msg::DnsClass::NONE => dns::rr::DNSClass::NONE,
        msg::DnsClass::ANY => dns::rr::DNSClass::ANY,
      };

      let rdata: RData = match ans.rdata_type() {
        msg::DnsRecordData::DnsA => {
          let d = ans.rdata_as_dns_a().unwrap();
          RData::A(d.ip().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsAaaa => {
          let d = ans.rdata_as_dns_aaaa().unwrap();
          RData::AAAA(d.ip().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsCname => {
          let d = ans.rdata_as_dns_cname().unwrap();
          RData::CNAME(d.name().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsMx => {
          let d = ans.rdata_as_dns_mx().unwrap();
          RData::MX(dns::rr::rdata::mx::MX::new(
            d.preference(),
            d.exchange().unwrap().parse().unwrap(),
          ))
        }
        msg::DnsRecordData::DnsNs => {
          let d = ans.rdata_as_dns_ns().unwrap();
          RData::NS(d.name().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsPtr => {
          let d = ans.rdata_as_dns_ptr().unwrap();
          RData::PTR(d.name().unwrap().parse().unwrap())
        }
        msg::DnsRecordData::DnsSoa => {
          let d = ans.rdata_as_dns_soa().unwrap();
          RData::SOA(dns::rr::rdata::soa::SOA::new(
            d.mname().unwrap().parse().unwrap(),
            d.rname().unwrap().parse().unwrap(),
            d.serial(),
            d.refresh(),
            d.retry(),
            d.expire(),
            d.minimum(),
          ))
        }
        msg::DnsRecordData::DnsSrv => {
          let d = ans.rdata_as_dns_srv().unwrap();
          RData::SRV(dns::rr::rdata::srv::SRV::new(
            d.priority(),
            d.weight(),
            d.port(),
            d.target().unwrap().parse().unwrap(),
          ))
        }
        msg::DnsRecordData::DnsTxt => {
          let d = ans.rdata_as_dns_txt().unwrap();
          let tdata = d.data().unwrap();
          let data_len = tdata.len();
          let mut txtdata: Vec<String> = Vec::with_capacity(data_len);
          for i in 0..data_len {
            let td = tdata.get(i);
            txtdata.push(String::from_utf8_lossy(td.data().unwrap()).to_string());
          }
          RData::TXT(dns::rr::rdata::txt::TXT::new(txtdata))
        }
        _ => unimplemented!(),
      };

      answers.push(JsDnsRecord {
        name: ans.name().unwrap().parse().unwrap(),
        dns_class: dns_class,
        ttl: ans.ttl(),
        rdata: rdata,
      });
    }
    answers
  } else {
    vec![]
  };

  let rt = ptr.to_runtime();

  let mut responses = rt.dns_responses.lock().unwrap();
  match responses.remove(&req_id) {
    Some(sender) => {
      if let Err(_) = sender.send(JsDnsResponse {
        op_code: op_code,
        authoritative: msg.authoritative(),
        truncated: msg.truncated(),
        response_code: res_code.into(),
        message_type: message_type,
        queries: queries,
        answers: answers,
      }) {
        return odd_future("error sending dns response".to_string().into());
      }
    }
    None => return odd_future("no dns response receiver!".to_string().into()),
  };

  ok_future(None)
}
