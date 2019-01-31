/**
 * @module fetch
 */
import { RequestInit, RequestInfo } from './dom_types';
import { FlyResponse } from './response';
import { FlyRequest } from './request';
import { sendAsync, streams, sendStreamChunks } from './bridge';

import * as fbs from "./msg_generated";
import * as flatbuffers from "./flatbuffers"
import { ReadableStream } from '@stardazed/streams';

import { libfly } from './libfly';

export interface FlyRequestInit extends RequestInit {
	timeout?: number,
	readTimeout?: number
}

const fbsMethodMap: Map<String, fbs.HttpMethod> = new Map([
	["GET", fbs.HttpMethod.Get],
	["HEAD", fbs.HttpMethod.Head],
	["POST", fbs.HttpMethod.Post],
	["PUT", fbs.HttpMethod.Put],
	["PATCH", fbs.HttpMethod.Patch],
	["DELETE", fbs.HttpMethod.Delete],
	["CONNECT", fbs.HttpMethod.Connect],
	["OPTIONS", fbs.HttpMethod.Options],
	["TRACE", fbs.HttpMethod.Trace],
]);

/**
 * Starts the process of fetching a network request.
 * 
 * See https://developer.mozilla.org/en-US/docs/Web/API/WindowOrWorkerGlobalScope/fetch
 * @global
 * @param req - The direct URL or Request for the resource you wish to fetch
 * @param init - Options for the request
 * @return A Promise that resolves to a {@linkcode Response} object
 */

export function fetch(info: RequestInfo, init?: FlyRequestInit): Promise<FlyResponse> {
	const req = new FlyRequest(info, init)
	const url = req.url

	if (!url)
		throw new Error("fetch url required")

	let fbbMethod = fbsMethodMap.get(req.method.toUpperCase());
	if (typeof fbbMethod === "undefined")
		throw new Error(`unknown http method: ${req.method}`);
	const fbb = flatbuffers.createBuilder();
	const urlStr = fbb.createString(url);

	let fbbHeaders = Array<number>();

	let i = 0;
	for (const header of req.headers) {
		const key = fbb.createString(header[0]);
		const value = fbb.createString(header[1]);
		fbs.HttpHeader.startHttpHeader(fbb);
		fbs.HttpHeader.addKey(fbb, key);
		fbs.HttpHeader.addValue(fbb, value);
		fbbHeaders[i++] = fbs.HttpHeader.endHttpHeader(fbb);
	}
	let reqHeaders = fbs.HttpRequest.createHeadersVector(fbb, fbbHeaders);
	fbs.HttpRequest.startHttpRequest(fbb);
	const reqId = libfly.getNextStreamId();
	fbs.HttpRequest.addId(fbb, reqId);
	fbs.HttpRequest.addMethod(fbb, fbbMethod);
	fbs.HttpRequest.addUrl(fbb, urlStr);
	fbs.HttpRequest.addHeaders(fbb, reqHeaders);

	let reqBody = req.body;
	let hasBody = reqBody != null && (!req.isStatic || req.isStatic && req.staticBody.byteLength > 0);
	fbs.HttpRequest.addHasBody(fbb, hasBody);

	let staticBody: BufferSource;
	if (hasBody && req.isStatic)
		staticBody = req.staticBody

	const prom = sendAsync(fbb, fbs.Any.HttpRequest, fbs.HttpRequest.endHttpRequest(fbb), staticBody).then((base) => {
		let msg = new fbs.FetchHttpResponse();
		base.msg(msg);
		const body = msg.hasBody() ?
			new ReadableStream({
				start(controller) {
					streams.set(msg.id(), (chunkMsg: fbs.StreamChunk, raw: Uint8Array) => {
						controller.enqueue(raw);
						if (chunkMsg.done()) {
							controller.close()
							streams.delete(chunkMsg.id())
						}
					})
				}
			}) : null
		const headersInit: Array<[string, string]> = [];
		for (let i = 0; i < msg.headersLength(); i++) {
			const h = msg.headers(i);
			headersInit.push([h.key(), h.value()]);
		}

		return new FlyResponse(body, { headers: headersInit, status: msg.status() })
	});

	if (!staticBody && hasBody) // must be a stream
		sendStreamChunks(reqId, reqBody) // don't wait for it, just start sending.

	return prom
};

export class TimeoutError extends Error { }