/**
 * @module fetch
 */
// import { logger } from './logger'
// import refToStream, { isFlyStream } from './fly/streams'
import { RequestInit, RequestInfo, HeadersInit } from './dom_types';
import { FlyResponse } from './response';
import { FlyRequest } from './request';
import { sendAsync, sendSync, streams } from './bridge';

import * as fbs from "./msg_generated";
import * as errors from "./errors";
import * as util from "./util";
import * as flatbuffers from "./flatbuffers"
import { ReadableStream } from '@stardazed/streams';


export interface FlyRequestInit extends RequestInit {
	timeout?: number,
	readTimeout?: number
}

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
	fbs.HttpRequest.addUrl(fbb, urlStr);

	let method: fbs.HttpMethod;
	switch (req.method) {
		case "GET":
			method = fbs.HttpMethod.Get;
			break;
		case "HEAD":
			method = fbs.HttpMethod.Head;
			break;
	}

	fbs.HttpRequest.addMethod(fbb, method);
	fbs.HttpRequest.addHeaders(fbb, reqHeaders);
	return sendAsync(fbb, fbs.Any.HttpRequest, fbs.HttpRequest.endHttpRequest(fbb)).then((base) => {
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
};

export class TimeoutError extends Error { }