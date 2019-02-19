/**
 * @module fly
 * @private
 */
import { libfly } from "./libfly";
import * as flatbuffers from "./flatbuffers";
import * as fbs from "./msg_generated";
import * as errors from "./errors";
import * as util from "./util";
import FlyBody from "./body_mixin";
import { FlyRequest } from "./request";
import { Response, ResponseInit } from "./dom_types";
import { FlyResponse } from "./response";
import { ReadableStream, ReadableStreamSource, StreamStrategy } from "@stardazed/streams";
import { DNSRequest, DNSQuery, DNSResponse, DNSDataA, DNSDataAAAA, DNSDataCNAME, DNSDataMX, DNSDataNS, DNSDataPTR, DNSDataSOA, DNSDataSRV, DNSDataTXT } from './dns';
import { isAcmeChallengeRequest, handleAcmeChallenge } from "./acme";

let nextCmdId = 1; // 0 is for events
const promiseTable = new Map<number, util.Resolvable<fbs.Base>>();
const listenerTable = new Map<fbs.Any, Function>();
export const streams = new Map<number, (msg: fbs.StreamChunk, raw: Uint8Array) => void>();

export function handleAsyncMsgFromRust(ui8: Uint8Array, raw: Uint8Array) {
  const bb = new flatbuffers.ByteBuffer(ui8);
  const base = fbs.Base.getRootAsBase(bb);
  const cmdId = base.cmdId();
  if (cmdId != 0)
    return handleCmdResponse(cmdId, base);

  handleEvent(base, raw);
}

function handleCmdResponse(cmdId: number, base: fbs.Base) {
  const promise = promiseTable.get(cmdId);
  // util.assert(promise != null, `Expecting promise in table. ${cmdId}`);
  if (!promise)
    return
  promiseTable.delete(cmdId);
  const err = errors.maybeError(base);
  if (err != null) {
    promise!.reject(err);
  } else {
    promise!.resolve(base);
  }
}

function handleEvent(base: fbs.Base, raw: Uint8Array) {
  const type = base.msgType();
  switch (type) {
    case fbs.Any.StreamChunk:
      handleBody(base, raw);
      break;
    default:
      const ln = listenerTable.get(type);
      if (!ln) {
        console.warn("unhandled event:", fbs.Any[type]);
        return
      }
      ln.call(window, base);
  }
}

function handleBody(base: fbs.Base, raw: Uint8Array) {
  let msg = new fbs.StreamChunk();
  base.msg(msg);

  let enqueuer = streams.get(msg.id())
  if (enqueuer)
    enqueuer(msg, raw)
}

export type DNSResponseFn = () => DNSResponse | Promise<DNSResponse>

export type EventResponse<REST> = () => REST | Promise<REST> | REST;

export interface RequestEvent<REST, REQT> {
  respondWith: (this: typeof window, resp: EventResponse<REST>) => void;
  request: REQT,
}

export interface DnsRequestEvent extends RequestEvent<DNSResponse, DNSRequest> {
}

export interface HttpRequestEvent extends RequestEvent<Response, FlyRequest> {
}

export type EventListenerFunction<ET> = (event: ET) => void;

export function addEventListener(name: "fetch", fn: EventListenerFunction<HttpRequestEvent>);
export function addEventListener(name: "resolve", fn: EventListenerFunction<DnsRequestEvent>);
export function addEventListener(name: string, fn: EventListenerFunction<RequestEvent<any, any>>) {
  let event_type: fbs.EventType;
  switch (name) {
    case "fetch":
      listenerTable.set(fbs.Any.HttpRequest, function (base: fbs.Base) {
        let msg = new fbs.HttpRequest();
        base.msg(msg);
        let id = msg.id();

        const headersInit: Array<[string, string]> = [];
        // console.log("headers len:", msg.headersLength());
        for (let i = 0; i < msg.headersLength(); i++) {
          const h = msg.headers(i);
          // console.log("header:", h.key(), h.value());
          // Not null operators to appease the typescript gods. These should never be null as far as I can tell.
          headersInit.push([h!.key()!, h!.value()!]);
        }

        let req = new FlyRequest(msg.url(), {
          method: fbs.HttpMethod[msg.method()].toUpperCase(),
          headers: headersInit,
          body: msg.hasBody() ?
            new ReadableStream({
              start(controller) {
                streams.set(id, (chunkMsg: fbs.StreamChunk, raw: Uint8Array) => {
                  // console.log("got a chunk:", chunkMsg.bytesArray());
                  controller.enqueue(raw);
                  if (chunkMsg.done()) {
                    controller.close()
                    streams.delete(chunkMsg.id())
                  }
                })
              }
            }) : null
        })

        req.remoteAddr = msg.remoteAddr();

        if (isAcmeChallengeRequest(req)) {
          handleAcmeChallenge(req)
            .then(res => handleRes(id, res))
            .catch(err => handleError(id, err));
        } else {
          try {
            fn.call(window, {
              request: req,
              respondWith(resfn: any) {
                try {
                  let ret = resfn;
                  if (typeof ret === "function") {
                    ret = resfn()
                  }
                  if (ret instanceof Promise) {
                    ret.then(handleRes.bind(null, id)).catch(handleError.bind(null, id))
                  } else if (ret instanceof Response) {
                    handleRes(id, ret)
                  }
                } catch (e) {
                  console.log("error in fetch event respondWith")
                  handleError(id, e)
                }
              }
            })
          } catch (e) {
            console.log("error in fetch event handler function")
            handleError(id, e)
          }
        }
      })
      event_type = fbs.EventType.Fetch;
      break;

    case "resolv": {
      listenerTable.set(fbs.Any.DnsRequest, function (base: fbs.Base) {
        let msg = new fbs.DnsRequest();
        base.msg(msg);
        let id = msg.id();

        let q = msg.queries(0);

        const req = new DNSRequest(q.name(), { type: q.rrType() })

        try {
          fn.call(window, {
            request: req,
            respondWith(resfn: any) {//DNSResponse | Promise<DNSResponse> | DNSResponseFn) {
              try {
                let ret = resfn;
                if (typeof ret === "function") {
                  ret = resfn()
                }
                if (ret instanceof Promise) {
                  ret.then(handleDNSRes.bind(null, id)).catch(handleDNSError.bind(null, id))
                } else if (ret instanceof DNSResponse) {
                  handleDNSRes(id, ret)
                }
              } catch (e) {
                console.log("error in resolv event respondWith")
                handleDNSError(id, e)
              }
            }
          })
        } catch (e) {
          console.log("error in resolv event handler function")
          handleDNSError(id, e)
        }
      })
      event_type = fbs.EventType.Resolv;
      break;
    }
  }
  const fbb = flatbuffers.createBuilder();
  fbs.AddEventListener.startAddEventListener(fbb);
  fbs.AddEventListener.addEvent(fbb, event_type);
  sendSync(fbb, fbs.Any.AddEventListener, fbs.AddEventListener.endAddEventListener(fbb))
}

function handleDNSError(id: number, err: Error) {
  console.error("dns error:", err.stack);
  const fbb = flatbuffers.createBuilder();

  fbs.DnsResponse.startDnsResponse(fbb);
  fbs.DnsResponse.addId(fbb, id);
  fbs.DnsResponse.addOpCode(fbb, fbs.DnsOpCode.Query)
  fbs.DnsResponse.addMessageType(fbb, fbs.DnsMessageType.Response)
  fbs.DnsResponse.addResponseCode(fbb, fbs.DnsResponseCode.ServFail)
  fbs.DnsResponse.addAuthoritative(fbb, true)

  sendAsync(fbb, fbs.Any.DnsResponse, fbs.DnsResponse.endDnsResponse(fbb));
}

function handleDNSRes(id: number, res: DNSResponse) {
  const fbb = flatbuffers.createBuilder();

  let answers: number[] = []
  for (let i = 0; i < res.answers.length; i++) {
    const ans = res.answers[i];
    let rdata: flatbuffers.Offset;
    let rdataType: fbs.DnsRecordData;
    switch (ans.type) {
      case fbs.DnsRecordType.A: {
        rdataType = fbs.DnsRecordData.DnsA
        const ip = fbb.createString((<DNSDataA>ans.data).ip)
        fbs.DnsA.startDnsA(fbb)
        fbs.DnsA.addIp(fbb, ip)
        rdata = fbs.DnsA.endDnsA(fbb)
        break;
      }
      case fbs.DnsRecordType.AAAA: {
        rdataType = fbs.DnsRecordData.DnsAaaa
        const ip = fbb.createString((<DNSDataAAAA>ans.data).ip)
        fbs.DnsAaaa.startDnsAaaa(fbb)
        fbs.DnsAaaa.addIp(fbb, ip)
        rdata = fbs.DnsAaaa.endDnsAaaa(fbb)
        break;
      }
      case fbs.DnsRecordType.CNAME: {
        rdataType = fbs.DnsRecordData.DnsCname
        const name = fbb.createString((<DNSDataCNAME>ans.data).name)
        fbs.DnsCname.startDnsCname(fbb)
        fbs.DnsCname.addName(fbb, name)
        rdata = fbs.DnsCname.endDnsCname(fbb)
        break;
      }
      case fbs.DnsRecordType.MX: {
        rdataType = fbs.DnsRecordData.DnsMx
        const data = <DNSDataMX>ans.data
        const ex = fbb.createString(data.exchange)
        fbs.DnsMx.startDnsMx(fbb)
        fbs.DnsMx.addPreference(fbb, data.preference)
        fbs.DnsMx.addExchange(fbb, ex)
        rdata = fbs.DnsMx.endDnsMx(fbb)
        break;
      }
      case fbs.DnsRecordType.NS: {
        rdataType = fbs.DnsRecordData.DnsNs
        const name = fbb.createString((<DNSDataNS>ans.data).name)
        fbs.DnsNs.startDnsNs(fbb)
        fbs.DnsNs.addName(fbb, name)
        rdata = fbs.DnsNs.endDnsNs(fbb)
        break;
      }
      case fbs.DnsRecordType.PTR: {
        rdataType = fbs.DnsRecordData.DnsPtr
        const name = fbb.createString((<DNSDataPTR>ans.data).name)
        fbs.DnsPtr.startDnsPtr(fbb)
        fbs.DnsPtr.addName(fbb, name)
        rdata = fbs.DnsPtr.endDnsPtr(fbb)
        break;
      }
      case fbs.DnsRecordType.SOA: {
        rdataType = fbs.DnsRecordData.DnsSoa
        const data = <DNSDataSOA>ans.data
        const mname = fbb.createString(data.mname)
        const rname = fbb.createString(data.rname)
        fbs.DnsSoa.startDnsSoa(fbb)
        fbs.DnsSoa.addMname(fbb, mname)
        fbs.DnsSoa.addRname(fbb, rname)
        fbs.DnsSoa.addSerial(fbb, data.serial)
        fbs.DnsSoa.addRefresh(fbb, data.refresh)
        fbs.DnsSoa.addRetry(fbb, data.retry)
        fbs.DnsSoa.addExpire(fbb, data.expire)
        fbs.DnsSoa.addMinimum(fbb, data.minimum)
        rdata = fbs.DnsSoa.endDnsSoa(fbb)
        break;
      }
      case fbs.DnsRecordType.SRV: {
        rdataType = fbs.DnsRecordData.DnsSrv
        const data = <DNSDataSRV>ans.data
        const target = fbb.createString(data.target)
        fbs.DnsSrv.startDnsSrv(fbb)
        fbs.DnsSrv.addPriority(fbb, data.priority)
        fbs.DnsSrv.addWeight(fbb, data.weight)
        fbs.DnsSrv.addPort(fbb, data.port)
        fbs.DnsSrv.addTarget(fbb, target)
        rdata = fbs.DnsSrv.endDnsSrv(fbb)
        break;
      }
      case fbs.DnsRecordType.TXT: {
        rdataType = fbs.DnsRecordData.DnsTxt
        const data = <DNSDataTXT>ans.data
        const txtData = fbs.DnsTxt.createDataVector(fbb, data.data.map(bytes => {
          const txtDataInner = fbs.DnsTxtData.createDataVector(fbb, bytes)
          fbs.DnsTxtData.startDnsTxtData(fbb)
          fbs.DnsTxtData.addData(fbb, txtDataInner)
          return fbs.DnsTxtData.endDnsTxtData(fbb)
        }))
        fbs.DnsTxt.startDnsTxt(fbb)
        fbs.DnsTxt.addData(fbb, txtData)
        rdata = fbs.DnsTxt.endDnsTxt(fbb)
        break;
      }
      default:
        throw new Error("unhandled record type: " + fbs.DnsRecordType[ans.type])
    }

    const name = fbb.createString(ans.name);
    fbs.DnsRecord.startDnsRecord(fbb);
    fbs.DnsRecord.addName(fbb, name);
    fbs.DnsRecord.addRdataType(fbb, rdataType);
    fbs.DnsRecord.addRdata(fbb, rdata);
    fbs.DnsRecord.addRrType(fbb, ans.type);
    fbs.DnsRecord.addTtl(fbb, ans.ttl);
    answers[i] = fbs.DnsRecord.endDnsRecord(fbb);
  }
  const answersOffset = fbs.DnsResponse.createAnswersVector(fbb, answers);

  fbs.DnsResponse.startDnsResponse(fbb);
  fbs.DnsResponse.addId(fbb, id);
  fbs.DnsResponse.addOpCode(fbb, fbs.DnsOpCode.Query) // override
  fbs.DnsResponse.addMessageType(fbb, fbs.DnsMessageType.Response) // override
  if (res.responseCode > 0)
    fbs.DnsResponse.addResponseCode(fbb, res.responseCode)
  fbs.DnsResponse.addAuthoritative(fbb, !!res.authoritative)
  fbs.DnsResponse.addTruncated(fbb, !!res.truncated)
  fbs.DnsResponse.addAnswers(fbb, answersOffset);
  sendAsync(fbb, fbs.Any.DnsResponse, fbs.DnsResponse.endDnsResponse(fbb));
}

function handleError(id: number, err: Error) {
  const fbb = flatbuffers.createBuilder();

  fbs.HttpResponse.startHttpResponse(fbb);
  fbs.HttpResponse.addId(fbb, id);
  fbs.HttpResponse.addHasBody(fbb, true)
  fbs.HttpResponse.addStatus(fbb, 500)

  const resMsg = fbs.HttpResponse.endHttpResponse(fbb);
  sendSync(fbb, fbs.Any.HttpResponse, resMsg);
  sendStreamChunk(id, true, new TextEncoder().encode(err.stack));
}

export async function sendStreamChunks(id: number, stream: ReadableStream) {
  let reader = stream.getReader();
  let cur = await reader.read()
  let done = false
  while (!done) {
    let value: BufferSource;
    if (typeof cur.value === 'string')
      value = new TextEncoder().encode(cur.value)
    else if (cur.value instanceof Uint8Array || cur.value instanceof ArrayBuffer)
      value = cur.value
    else if (typeof cur.value === 'undefined' || cur.value === null)
      value = undefined
    else
      throw new TypeError(`wrong body type: ${typeof cur.value} -> ${cur.value}`)
    sendStreamChunk(id, cur.done, value);
    if (cur.done)
      done = true
    else
      cur = await reader.read()
  }
}

export function sendStreamChunk(id: number, done: boolean, value?: BufferSource) {
  const fbb = flatbuffers.createBuilder()
  fbs.StreamChunk.startStreamChunk(fbb)
  fbs.StreamChunk.addId(fbb, id);
  fbs.StreamChunk.addDone(fbb, done);
  sendSync(fbb, fbs.Any.StreamChunk, fbs.StreamChunk.endStreamChunk(fbb), value)
}

async function handleRes(id: number, res: FlyResponse) {
  if (res.bodyUsed)
    throw new Error("BODY HAS BEEN USED, NO PUEDO!")
  // console.log("respond with!", res);

  const fbb = flatbuffers.createBuilder();

  let fbbHeaders = Array<number>();

  try {
    // console.log("trying stuff")
    let i = 0;
    for (const [n, v] of res.headers) {
      const key = fbb.createString(n);
      const value = fbb.createString(v);
      fbs.HttpHeader.startHttpHeader(fbb);
      fbs.HttpHeader.addKey(fbb, key);
      fbs.HttpHeader.addValue(fbb, value);
      fbbHeaders[i++] = fbs.HttpHeader.endHttpHeader(fbb);
    }
    // console.log(fbbHeaders);
    let resHeaders = fbs.HttpResponse.createHeadersVector(fbb, fbbHeaders);

    fbs.HttpResponse.startHttpResponse(fbb);
    fbs.HttpResponse.addId(fbb, id);
    fbs.HttpResponse.addHeaders(fbb, resHeaders);
    fbs.HttpResponse.addStatus(fbb, res.status);
    let resBody = res.body;
    let hasBody = resBody != null && (!res.isStatic || res.isStatic && res.staticBody.byteLength > 0)
    fbs.HttpResponse.addHasBody(fbb, hasBody)

    const resMsg = fbs.HttpResponse.endHttpResponse(fbb);

    let staticBody: BufferSource;
    if (hasBody && res.isStatic)
      staticBody = res.staticBody
    sendSync(fbb, fbs.Any.HttpResponse, resMsg, staticBody); // sync so we can send body chunks when it's ready!

    if (staticBody || !hasBody)
      return
    await sendStreamChunks(id, resBody);

  } catch (e) {
    console.log("caught an error:", e.message, e.stack);
    throw e
  }
}

// @internal
export function sendAsync(
  fbb: flatbuffers.Builder,
  msgType: fbs.Any,
  msg: flatbuffers.Offset,
  raw?: BufferSource
): Promise<fbs.Base> {
  const [cmdId, resBuf] = sendInternal(fbb, msgType, msg, false, raw);
  util.assert(resBuf == null);
  const promise = util.createResolvable<fbs.Base>();
  promiseTable.set(cmdId, promise);
  return promise;
}

// @internal
export function sendSync(
  fbb: flatbuffers.Builder,
  msgType: fbs.Any,
  msg: flatbuffers.Offset,
  raw?: BufferSource
): null | fbs.Base {
  const [cmdId, resBuf] = sendInternal(fbb, msgType, msg, true, raw);
  util.assert(cmdId >= 0);
  if (resBuf == null) {
    return null;
  } else {
    const u8 = new Uint8Array(resBuf!);
    const bb = new flatbuffers.ByteBuffer(u8);
    const baseRes = fbs.Base.getRootAsBase(bb);
    errors.maybeThrowError(baseRes);
    return baseRes;
  }
}

function sendInternal(
  fbb: flatbuffers.Builder,
  msgType: fbs.Any,
  msg: flatbuffers.Offset,
  sync = true,
  raw?: BufferSource
): [number, null | Uint8Array] {
  const cmdId = nextCmdId++;
  fbs.Base.startBase(fbb);
  fbs.Base.addMsg(fbb, msg);
  fbs.Base.addMsgType(fbb, msgType);
  fbs.Base.addSync(fbb, sync);
  fbs.Base.addCmdId(fbb, cmdId);
  fbb.finish(fbs.Base.endBase(fbb));

  const res = libfly.send(fbb.asUint8Array(), raw);
  fbb.inUse = false;
  return [cmdId, res];
}
