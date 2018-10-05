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
import { flatbuffers } from "flatbuffers"
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
	return new Promise(function fetchPromise(resolve, reject) {
		try {
			const req = new FlyRequest(info, init)
			const url = req.url

			const fbb = new flatbuffers.Builder();
			const urlStr = fbb.createString(url);

			let headersArr = Array.from(req.headers[Symbol.iterator]());
			const headersLength = headersArr.length;
			let fbbHeaders = Array<number>();

			for (let i = 0; i < headersLength; i++) {
				const key = fbb.createString(headersArr[i].name);
				const value = fbb.createString(headersArr[i].value);
				fbs.HttpHeader.startHttpHeader(fbb);
				fbs.HttpHeader.addKey(fbb, key);
				fbs.HttpHeader.addValue(fbb, value);
				fbbHeaders[i] = fbs.HttpHeader.endHttpHeader(fbb);
			}
			let reqHeaders = fbs.HttpRequest.createHeadersVector(fbb, fbbHeaders);
			fbs.HttpRequest.startHttpRequest(fbb);
			fbs.HttpRequest.addUrl(fbb, urlStr);
			fbs.HttpRequest.addMethod(fbb, fbs.HttpMethod.Get);
			fbs.HttpRequest.addHeaders(fbb, reqHeaders);
			sendAsync(fbb, fbs.Any.HttpRequest, fbs.HttpRequest.endHttpRequest(fbb)).then((base) => {
				let msg = new fbs.FetchHttpResponse();
				base.msg(msg);
				const body = msg.body() ?
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
				const headersInit: string[][] = [];
				for (let i = 0; i < msg.headersLength(); i++) {
					const h = msg.headers(i);
					headersInit.push([h.key(), h.value()]);
				}

				resolve(new FlyResponse(body, { headers: headersInit, status: msg.status() }))
			}).catch(reject);

		} catch (err) {
			reject(err)
		}
	})
};

export class TimeoutError extends Error { }