use crate::msg;
use flatbuffers::FlatBufferBuilder;

use trust_dns::client::ClientHandle; // necessary for trait to be in scope
use trust_dns::proto as dns_proto;
use trust_dns_resolver::config::ResolverConfig;

use std::collections::HashMap;
use std::sync::Mutex;

use crate::runtime::{Runtime, EVENT_LOOP};
use crate::utils::*;
use libfly::*;

use futures::Future;

use std::net::{SocketAddr, ToSocketAddrs};

use crate::js::*;

lazy_static! {
  static ref DEFAULT_RESOLVER_CONFIG: ResolverConfig = {
    match trust_dns_resolver::system_conf::read_system_conf() {
      Ok((r, _)) => r,
      Err(e) => {
        warn!("error getting system resolv conf: {}, using google's", e);
        ResolverConfig::google()
      }
    }
  };
  static ref DEFAULT_RESOLVER: Mutex<trust_dns::client::BasicClientHandle<dns_proto::udp::UdpResponse>> = {
    let stream =
      trust_dns::udp::UdpClientStream::new(DEFAULT_RESOLVER_CONFIG.name_servers()[0].socket_addr);
    let (bg, client) = trust_dns::client::ClientFuture::connect(stream);
    EVENT_LOOP.0.spawn(bg);
    Mutex::new(client)
  };
  static ref DNS_RESOLVERS: Mutex<HashMap<SocketAddr, trust_dns::client::BasicClientHandle<dns_proto::udp::UdpResponse>>> =
    Mutex::new(HashMap::new());
}

fn dns_query(
  cmd_id: u32,
  client: &mut trust_dns::client::BasicClientHandle<dns_proto::udp::UdpResponse>,
  name: &str,
  query_type: trust_dns::rr::RecordType,
) -> Box<Op> {
  debug!("dns_query {} {}", cmd_id, name);
  Box::new(
    client
      .query(
        name.parse().unwrap(),
        trust_dns::rr::DNSClass::IN,
        query_type,
      )
      .map_err(|e| format!("dns query error: {}", e).into())
      .and_then(move |res| {
        // debug!("got a dns response! {:?}", res);
        for q in res.queries() {
          debug!("queried: {:?}", q);
        }
        let builder = &mut FlatBufferBuilder::new();
        let answers: Vec<_> = res
          .answers()
          .iter()
          .map(|ans| {
            debug!("answer: {:?}", ans);
            use trust_dns::rr::{DNSClass, RData, RecordType};
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
                )
                .as_union_value()
              }
              RData::AAAA(ip) => {
                let ipstr = builder.create_string(&ip.to_string());
                msg::DnsAaaa::create(
                  builder,
                  &msg::DnsAaaaArgs {
                    ip: Some(ipstr),
                    ..Default::default()
                  },
                )
                .as_union_value()
              }
              RData::CNAME(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsCname::create(
                  builder,
                  &msg::DnsCnameArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                )
                .as_union_value()
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
                )
                .as_union_value()
              }
              RData::NS(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsNs::create(
                  builder,
                  &msg::DnsNsArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                )
                .as_union_value()
              }
              RData::PTR(name) => {
                let namestr = builder.create_string(&name.to_utf8());
                msg::DnsPtr::create(
                  builder,
                  &msg::DnsPtrArgs {
                    name: Some(namestr),
                    ..Default::default()
                  },
                )
                .as_union_value()
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
                )
                .as_union_value()
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
                )
                .as_union_value()
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
                  })
                  .collect();
                let data = builder.create_vector(&coll);
                msg::DnsTxt::create(
                  builder,
                  &msg::DnsTxtArgs {
                    data: Some(data),
                    ..Default::default()
                  },
                )
                .as_union_value()
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
          })
          .collect();
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

pub fn op_dns_query(_rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  debug!("handle dns");
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_dns_query().unwrap();

  let query_type = match msg.rr_type() {
    msg::DnsRecordType::A => trust_dns::rr::RecordType::A,
    msg::DnsRecordType::AAAA => trust_dns::rr::RecordType::AAAA,
    msg::DnsRecordType::ANY => trust_dns::rr::RecordType::ANY,
    msg::DnsRecordType::AXFR => trust_dns::rr::RecordType::AXFR,
    msg::DnsRecordType::CAA => trust_dns::rr::RecordType::CAA,
    msg::DnsRecordType::CNAME => trust_dns::rr::RecordType::CNAME,
    msg::DnsRecordType::IXFR => trust_dns::rr::RecordType::IXFR,
    msg::DnsRecordType::MX => trust_dns::rr::RecordType::MX,
    msg::DnsRecordType::NS => trust_dns::rr::RecordType::NS,
    msg::DnsRecordType::NULL => trust_dns::rr::RecordType::NULL,
    msg::DnsRecordType::OPT => trust_dns::rr::RecordType::OPT,
    msg::DnsRecordType::PTR => trust_dns::rr::RecordType::PTR,
    msg::DnsRecordType::SOA => trust_dns::rr::RecordType::SOA,
    msg::DnsRecordType::SRV => trust_dns::rr::RecordType::SRV,
    msg::DnsRecordType::TLSA => trust_dns::rr::RecordType::TLSA,
    msg::DnsRecordType::TXT => trust_dns::rr::RecordType::TXT,
  };

  let name = msg.name().unwrap();

  if let Some(nss) = msg.name_servers() {
    let ns = {
      let ns = nss.get(0);
      if ns.contains(":") {
        ns.to_string()
      } else {
        format!("{}:53", ns)
      }
    };
    let sockaddr = ns.to_socket_addrs().unwrap().next().unwrap();
    {
      if let Some(client) = DNS_RESOLVERS.lock().unwrap().get_mut(&sockaddr) {
        return dns_query(cmd_id, client, name, query_type);
      }
    }
    let stream = trust_dns::udp::UdpClientStream::new(sockaddr.clone());
    let (bg, client) = trust_dns::client::ClientFuture::connect(stream);
    EVENT_LOOP.0.spawn(bg);
    {
      DNS_RESOLVERS
        .lock()
        .unwrap()
        .insert(sockaddr.clone(), client);
    }
    debug!("INSERTED DNS RESOLVER");
    dns_query(
      cmd_id,
      DNS_RESOLVERS.lock().unwrap().get_mut(&sockaddr).unwrap(),
      name,
      query_type,
    )
  // }
  } else {
    let mut client = DEFAULT_RESOLVER.lock().unwrap();
    dns_query(cmd_id, &mut client, name, query_type)
  }
}

pub fn op_dns_response(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_dns_response().unwrap();
  let req_id = msg.id();

  let op_code = match msg.op_code() {
    msg::DnsOpCode::Query => trust_dns::op::OpCode::Query,
    msg::DnsOpCode::Status => trust_dns::op::OpCode::Status,
    msg::DnsOpCode::Notify => trust_dns::op::OpCode::Notify,
    msg::DnsOpCode::Update => trust_dns::op::OpCode::Update,
  };

  let res_code = msg.response_code() as u16;

  let message_type = match msg.message_type() {
    msg::DnsMessageType::Query => trust_dns::op::MessageType::Query,
    msg::DnsMessageType::Response => trust_dns::op::MessageType::Response,
  };

  use trust_dns::rr::RData;

  let queries: Vec<JsDnsQuery> = if let Some(msg_queries) = msg.queries() {
    let qlen = msg_queries.len();
    let mut queries: Vec<JsDnsQuery> = Vec::with_capacity(qlen);
    for i in 0..qlen {
      let q = msg_queries.get(i);

      let rr_type = match q.rr_type() {
        msg::DnsRecordType::A => trust_dns::rr::RecordType::A,
        msg::DnsRecordType::AAAA => trust_dns::rr::RecordType::AAAA,
        msg::DnsRecordType::ANY => trust_dns::rr::RecordType::ANY,
        msg::DnsRecordType::AXFR => trust_dns::rr::RecordType::AXFR,
        msg::DnsRecordType::CAA => trust_dns::rr::RecordType::CAA,
        msg::DnsRecordType::CNAME => trust_dns::rr::RecordType::CNAME,
        msg::DnsRecordType::IXFR => trust_dns::rr::RecordType::IXFR,
        msg::DnsRecordType::MX => trust_dns::rr::RecordType::MX,
        msg::DnsRecordType::NS => trust_dns::rr::RecordType::NS,
        msg::DnsRecordType::NULL => trust_dns::rr::RecordType::NULL,
        msg::DnsRecordType::OPT => trust_dns::rr::RecordType::OPT,
        msg::DnsRecordType::PTR => trust_dns::rr::RecordType::PTR,
        msg::DnsRecordType::SOA => trust_dns::rr::RecordType::SOA,
        msg::DnsRecordType::SRV => trust_dns::rr::RecordType::SRV,
        msg::DnsRecordType::TLSA => trust_dns::rr::RecordType::TLSA,
        msg::DnsRecordType::TXT => trust_dns::rr::RecordType::TXT,
      };

      let dns_class = match q.dns_class() {
        msg::DnsClass::IN => trust_dns::rr::DNSClass::IN,
        msg::DnsClass::CH => trust_dns::rr::DNSClass::CH,
        msg::DnsClass::HS => trust_dns::rr::DNSClass::HS,
        msg::DnsClass::NONE => trust_dns::rr::DNSClass::NONE,
        msg::DnsClass::ANY => trust_dns::rr::DNSClass::ANY,
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
        msg::DnsClass::IN => trust_dns::rr::DNSClass::IN,
        msg::DnsClass::CH => trust_dns::rr::DNSClass::CH,
        msg::DnsClass::HS => trust_dns::rr::DNSClass::HS,
        msg::DnsClass::NONE => trust_dns::rr::DNSClass::NONE,
        msg::DnsClass::ANY => trust_dns::rr::DNSClass::ANY,
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
          RData::MX(trust_dns::rr::rdata::mx::MX::new(
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
          RData::SOA(trust_dns::rr::rdata::soa::SOA::new(
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
          RData::SRV(trust_dns::rr::rdata::srv::SRV::new(
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
          RData::TXT(trust_dns::rr::rdata::txt::TXT::new(txtdata))
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
