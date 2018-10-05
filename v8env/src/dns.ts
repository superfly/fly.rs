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
  rrType: fbs.DnsRecordType,
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

export type DNSRecordData = string

export interface DNSRecord {
  name: string,
  rrType: fbs.DnsRecordType,
  dnsClass: fbs.DnsClass,
  ttl: number,
  data: DNSRecordData,
}

export interface DNSResponse {
  authoritative: boolean,
  truncated: boolean,
  responseCode: fbs.DnsResponseCode,
  queries: DNSQuery[],
  answers: DNSRecord[]
}

export interface DNSRequest {
  messageType: fbs.DnsMessageType,
  queries: DNSQuery[]
}