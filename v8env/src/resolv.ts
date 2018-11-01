import * as fbs from "./msg_generated";
import * as errors from "./errors";
import * as util from "./util";
import * as flatbuffers from "./flatbuffers"
import { sendAsync } from "./bridge";
import { DNSQuery, DNSResponse, DNSRecord, DNSRecordData, DNSRequest } from "./dns";
import { FlyResponse } from "./response";

export function resolv(info: string | DNSRequest, type?: fbs.DnsRecordType): Promise<DNSResponse> {
  let req: DNSRequest = typeof info === "string" ? new DNSRequest(info, type) : info

  return new Promise(function resolvPromise(resolve, reject) {
    const fbb = flatbuffers.createBuilder();
    const nameStr = fbb.createString(req.name)
    fbs.DnsQuery.startDnsQuery(fbb);
    fbs.DnsQuery.addName(fbb, nameStr);
    fbs.DnsQuery.addDnsClass(fbb, fbs.DnsClass.IN);
    fbs.DnsQuery.addRrType(fbb, req.type);
    sendAsync(fbb, fbs.Any.DnsQuery, fbs.DnsQuery.endDnsQuery(fbb)).then(baseRes => {
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
            data = { ip: d.ip() };
            break;
          }
          case fbs.DnsRecordData.DnsAaaa: {
            const d = new fbs.DnsAaaa()
            ans.rdata(d);
            data = { ip: d.ip() };
            break;
          }
          case fbs.DnsRecordData.DnsNs: {
            const d = new fbs.DnsNs()
            ans.rdata(d)
            data = { name: d.name() }
            break;
          }
          default:
            break;
          // return reject(new Error("unhandled record type: " + fbs.DnsRecordData[ans.type()]))
        }
        answers.push({
          name: ans.name(),
          type: ans.rrType(),
          dnsClass: ans.dnsClass(),
          ttl: ans.ttl(),
          data: data,
        })
      }
      resolve(new DNSResponse(answers, {
        authoritative: msg.authoritative(),
        truncated: msg.truncated(),
        responseCode: msg.responseCode(),
        queries: [{ name: req.name, type: req.type, dnsClass: fbs.DnsClass.IN }],
      }))
    }).catch(reject)
  })
}