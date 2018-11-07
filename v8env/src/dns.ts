import * as fbs from "./msg_generated";

export const DNSClass = {
  IN: fbs.DnsClass.IN,
  CH: fbs.DnsClass.CH,
  HS: fbs.DnsClass.HS,
  NONE: fbs.DnsClass.NONE,
  ANY: fbs.DnsClass.ANY,
}

export const DNSRecordType = {
  A: fbs.DnsRecordType.A,
  AAAA: fbs.DnsRecordType.AAAA,
  ANY: fbs.DnsRecordType.ANY,
  AXFR: fbs.DnsRecordType.AXFR,
  CAA: fbs.DnsRecordType.CAA,
  CNAME: fbs.DnsRecordType.CNAME,
  IXFR: fbs.DnsRecordType.IXFR,
  MX: fbs.DnsRecordType.MX,
  NS: fbs.DnsRecordType.NS,
  NULL: fbs.DnsRecordType.NULL,
  OPT: fbs.DnsRecordType.OPT,
  PTR: fbs.DnsRecordType.PTR,
  SOA: fbs.DnsRecordType.SOA,
  SRV: fbs.DnsRecordType.SRV,
  TLSA: fbs.DnsRecordType.TLSA,
  TXT: fbs.DnsRecordType.TXT,
}

export const DNSMessageType = {
  Query: fbs.DnsMessageType.Query,
  Response: fbs.DnsMessageType.Response,
}

export const DNSOpCode = {
  Query: fbs.DnsOpCode.Query,
  Status: fbs.DnsOpCode.Status,
  Notify: fbs.DnsOpCode.Notify,
  Update: fbs.DnsOpCode.Update
}

export const DNSResponseCode = {
  NoError: fbs.DnsResponseCode.NoError,
  FormErr: fbs.DnsResponseCode.FormErr,
  ServFail: fbs.DnsResponseCode.ServFail,
  NXDomain: fbs.DnsResponseCode.NXDomain,
  NotImp: fbs.DnsResponseCode.NotImp,
  Refused: fbs.DnsResponseCode.Refused,
  YXDomain: fbs.DnsResponseCode.YXDomain,
  YXRRSet: fbs.DnsResponseCode.YXRRSet,
  NXRRSet: fbs.DnsResponseCode.NXRRSet,
  NotAuth: fbs.DnsResponseCode.NotAuth,
  NotZone: fbs.DnsResponseCode.NotZone,
  BADVERS: fbs.DnsResponseCode.BADVERS,
  BADSIG: fbs.DnsResponseCode.BADSIG,
  BADKEY: fbs.DnsResponseCode.BADKEY,
  BADTIME: fbs.DnsResponseCode.BADTIME,
  BADMODE: fbs.DnsResponseCode.BADMODE,
  BADNAME: fbs.DnsResponseCode.BADNAME,
  BADALG: fbs.DnsResponseCode.BADALG,
  BADTRUNC: fbs.DnsResponseCode.BADTRUNC,
  BADCOOKIE: fbs.DnsResponseCode.BADCOOKIE,
}

export interface DNSQuery {
  name: string,
  dnsClass: fbs.DnsClass,
  type: fbs.DnsRecordType,
}

export interface DNSMessage {
  id: number,
  messageType: fbs.DnsMessageType,
  opCode: fbs.DnsOpCode,
  authoritative: boolean,
  truncated: boolean,
  responseCode: fbs.DnsResponseCode,
  queries: DNSQuery[],
  answers: DNSRecord[],
}

export interface DNSDataA {
  ip: string
}
export interface DNSDataAAAA {
  ip: string
}
export interface DNSDataCNAME {
  name: string
}
export interface DNSDataMX {
  preference: number
  exchange: string
}
export interface DNSDataNS {
  name: string
}
export interface DNSDataPTR {
  name: string
}
export interface DNSDataSOA {
  mname: string;
  rname: string;
  serial: number;
  refresh: number;
  retry: number;
  expire: number;
  minimum: number;
}
export interface DNSDataSRV {
  priority: number
  weight: number
  port: number
  target: string
}

export interface DNSDataTXT {
  data: Uint8Array[]
}

export type DNSRecordData = DNSDataA | DNSDataAAAA | DNSDataCNAME | DNSDataMX | DNSDataNS | DNSDataPTR | DNSDataSOA | DNSDataSRV | DNSDataTXT

export interface DNSRecord {
  name: string,
  type: fbs.DnsRecordType,
  dnsClass: fbs.DnsClass,
  ttl: number,
  data: DNSRecordData,
}

export interface DNSRequestInit {
  type?: fbs.DnsRecordType
  nameservers?: string[]
}

export class DNSRequest {
  name: string
  type: fbs.DnsRecordType
  nameservers: string[]
  constructor(name: string, init?: DNSRequestInit) {
    init || (init = {})
    this.name = name
    this.type = init.type || DNSRecordType.A
    this.nameservers = init.nameservers || []
  }
}

export interface DNSResponseInit {
  authoritative?: boolean
  truncated?: boolean
  responseCode?: fbs.DnsResponseCode
  queries?: DNSQuery[]
}

export class DNSResponse {
  authoritative: boolean
  truncated: boolean
  responseCode: fbs.DnsResponseCode
  answers: DNSRecord[]
  queries: DNSQuery[]

  constructor(answers: DNSRecord[], init?: DNSResponseInit) {
    this.answers = answers
    init || (init = {})
    this.authoritative = init.authoritative || false
    this.truncated = init.truncated || false
    this.responseCode = init.responseCode || fbs.DnsResponseCode.NoError
    this.queries = init.queries || []
  }
}