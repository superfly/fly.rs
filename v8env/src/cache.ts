/**
 * @module fly
 * @private
 */
// import { console } from './console'
import CachePolicy from 'http-cache-semantics'
import { Request, Response } from './dom_types';
import { FlyResponse } from './response';
import * as flyCache from "./fly/cache";

/**
 * export:
 * 	match(req): res | null
 * 	add(req): void
 * 	put(req, res): void
 */

const cache = {
	async match(req: Request) {
		const hashed = await hashData(req)
		let key = "httpcache:policy:" + hashed // first try with no vary variant
		for (let i = 0; i < 5; i++) {
			const policyRaw = await flyCache.getString(key)
			if (!policyRaw) {
				return undefined
			}
			try {
				const policy = CachePolicy.fromObject(JSON.parse(policyRaw))

				// if it fits i sits
				if (policy.satisfiesWithoutRevalidation(req)) {
					const headers = policy.responseHeaders()
					const bodyKey = "httpcache:body:" + hashed

					let body = await flyCache.getStream(bodyKey)
					return new FlyResponse(body, { status: policy._status, headers: headers })
					//}else if(policy._headers){
					// TODO: try a new vary based key
					// policy._headers has the varies / vary values
					// key = hashData(req, policy._headers)
					//return undefined
				} else {
					return undefined
				}
			} catch (e) {
				console.log("error!", e.message, e.stack)
				throw e
			}
			return undefined // no matches found
		}
	},
	add(req: Request) {
		return fetch(req).then(res => {
			return cache.put(req, res)
		})
	},
	async put(req: Request, res: FlyResponse) {
		const resHeaders = {}
		const key = await hashData(req)

		for (const [name, value] of res.headers) {
			// if (h.name === 'set-cookie')
			resHeaders[name] = value
			// else
			// 	resHeaders[h.name] = h[1].join && h[1].join(',') || h[1]
		}
		let cacheableRes = {
			status: res.status,
			headers: resHeaders,
		}
		const policy = new CachePolicy({
			url: req.url,
			method: req.method,
			headers: req.headers || {},
		}, cacheableRes)

		const ttl = Math.floor(policy.timeToLive() / 1000)
		if (policy.storable() && ttl > 0) {
			await flyCache.set("httpcache:policy:" + key, JSON.stringify(policy.toObject()), {ttl})
			await flyCache.set("httpcache:body:" + key, res.body, {ttl})
		}
	}
}

export default cache;

function hashData(req: Request, vary?) {
	let toHash = ``

	const u = normalizeURL(req.url)

	toHash += u.toString()
	toHash += req.method

	// TODO: cacheable cookies
	// TODO: cache version for grand busting

	var buffer = new TextEncoder("utf-8").encode(toHash);
	return crypto.subtle.digest("SHA-1", buffer).then(hash => {
		return hex(hash)
	}) // TODO: SHA-1, not 256
}

function normalizeURL(u) {
	let url = new URL(u)
	url.hash = ""
	const sp = url.searchParams
	sp.sort()
	url.search = sp.toString()

	return url
}

function hex(buff) {
	return [].map.call(new Uint8Array(buff), b => ('00' + b.toString(16)).slice(-2)).join('');
}