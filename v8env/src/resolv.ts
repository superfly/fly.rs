import * as fbs from "./msg_generated";
import * as errors from "./errors";
import * as util from "./util";
import { flatbuffers } from "flatbuffers"
import { sendAsync } from "./bridge";
import { DNSQuery, DNSResponse, DNSRecord, DNSRecordData } from "./dns";

export function resolv(req: DNSQuery | string): Promise<DNSResponse> {
  let query: DNSQuery = typeof req === "string" ? {
    name: req,
    rrType: fbs.DnsRecordType.A,
    dnsClass: fbs.DnsClass.IN
  } : req

  return new Promise(function resolvPromise(resolve, reject) {
    const fbb = new flatbuffers.Builder();
    const nameStr = fbb.createString(query.name)
    fbs.DnsQuery.startDnsQuery(fbb);
    fbs.DnsQuery.addName(fbb, nameStr);
    fbs.DnsQuery.addDnsClass(fbb, query.dnsClass);
    fbs.DnsQuery.addRrType(fbb, query.rrType);
    sendAsync(fbb, fbs.Any.DnsQuery, fbs.DnsQuery.endDnsQuery(fbb)).then(baseRes => {
      console.log("hello from resolv response")
      let msg = new fbs.DnsResponse()
      baseRes.msg(msg);
      const answers: DNSRecord[] = [];
      for (let i = 0; i < msg.answersLength(); i++) {
        const ans = msg.answers(i);
        console.log("parsing answer!", i, fbs.DnsRecordData[ans.rdataType()])
        let data: DNSRecordData;
        switch (ans.rdataType()) {
          case fbs.DnsRecordData.DnsA: {
            const d = new fbs.DnsA()
            ans.rdata(d);
            data = d.ip();
            break;
          }
          case fbs.DnsRecordData.DnsAAAA: {
            const d = new fbs.DnsAAAA()
            ans.rdata(d);
            data = d.ip();
            break;
          }
          case fbs.DnsRecordData.DnsNS: {
            const d = new fbs.DnsNS()
            ans.rdata(d)
            data = d.name()
            break;
          }
          default:
            break;
          // return reject(new Error("unhandled record type: " + fbs.DnsRecordData[ans.type()]))
        }
        answers.push({
          name: ans.name(),
          rrType: ans.rrType(),
          dnsClass: ans.dnsClass(),
          ttl: ans.ttl(),
          data: data,
        })
      }
      resolve({
        authoritative: msg.authoritative(),
        truncated: msg.truncated(),
        responseCode: msg.responseCode(),
        queries: [query],
        answers: answers
      })
    }).catch(reject)
  })
}