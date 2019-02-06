use crate::errors::FlyError;
use futures::{sync::mpsc, Stream};
use hyper::HeaderMap;
use hyper::StatusCode;
use std::net::SocketAddr;
use trust_dns as dns;

pub enum JsBody {
    BoxedStream(Box<Stream<Item = Vec<u8>, Error = FlyError> + Send>),
    Stream(mpsc::UnboundedReceiver<Vec<u8>>),
    Static(Vec<u8>),
}

pub struct JsHttpResponse {
    pub headers: HeaderMap,
    pub status: StatusCode,
    pub body: Option<JsBody>,
}

pub struct JsHttpRequest {
    pub id: u32,
    pub method: http::Method,
    pub remote_addr: SocketAddr,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<JsBody>,
}

pub enum JsEvent {
    Fetch(JsHttpRequest),
    Resolv(JsDnsRequest),
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
