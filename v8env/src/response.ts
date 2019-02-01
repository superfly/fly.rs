/**
 * @module fetch
 */
import CookieJar from './cookie_jar'
import FlyBody from './body_mixin'
import { FlyHeaders } from './headers';
import { Response, Headers, ResponseType, ResponseInit, BodyInit } from './dom_types';
import { ReadableStream } from '@stardazed/streams';

function ushort(x) { return x & 0xFFFF; }

/**
 * Class representing a fetch response.
 */
export class FlyResponse extends FlyBody implements Response {
	headers: FlyHeaders
	status: number
	url: string
	ok: boolean
	statusText: string
	type: ResponseType
	redirected: boolean
	trailer: Promise<Headers>
	private cookieJar: CookieJar

	static redirect(url, status = 302) {
		return new FlyResponse('', {
			status,
			headers: {
				Location: url
			}
		})
	}

	constructor(body: BodyInit, init: ResponseInit | FlyResponse) {
		if (arguments.length < 1)
			body = '';

		if (init instanceof FlyResponse)
			super(body || init.body)
		else
			super(body)

		init = Object(init) || {};

		/**
		 * @public
		 * @type {Headers}
		 */
		this.headers = new FlyHeaders(init.headers);

		// readonly attribute USVString url;
		/**
		 * @public
		 * @type {String}
		 * @readonly
		 */
		// this.url = init.url || '';

		// readonly attribute unsigned short status;
		var status = 'status' in init ? ushort(init.status) : 200;
		if (status < 200 || status > 599) throw RangeError();

		/**
		 * @public
		 * @type {integer}
		 * @readonly
		 */
		this.status = status;

		// readonly attribute boolean ok;
		/**
		 * @public
		 * @type {boolean}
		 * @readonly
		 */
		this.ok = 200 <= this.status && this.status <= 299;

		// readonly attribute ByteString statusText;
		var statusText = 'statusText' in init ? String(init.statusText) : 'OK';
		if (/[^\x00-\xFF]/.test(statusText)) throw TypeError();

		/**
		 * @public
		 * @type {String}
		 * @readonly
		 */
		this.statusText = statusText;

		// readonly attribute Headers headers;
		// if ('headers' in init) fill(this.headers, init);

		// TODO: Implement these
		// readonly attribute ResponseType type;
		this.type = 'basic'; // TODO: ResponseType

		// Object.defineProperty(this, "body", {
		//   set: (value) => {
		//     Body.call(this, value)
		//   }
		// })
	}

	/**
	 * @public
	 * @type CookieJar
	 */
	get cookies() {
		if (this.cookieJar)
			return this.cookieJar
		this.cookieJar = new CookieJar(this)
		return this.cookieJar
	}

	clone() {
		if (this.bodyUsed)
			throw new Error("Body has already been used")

		if (this.body instanceof ReadableStream) {
			const [b1, b2] = this.body.tee()
			this.setBody(b1)
			return new Response(b2, this)
		}
		return new Response(null, this)
	}
}
