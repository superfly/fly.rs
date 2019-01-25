/**
 * An API for efficiently caching Response objects in the regional Fly cache.
 * 
 * Usage:
 * 
 * ```javascript
 * import { responseCache } from "@fly/cache"
 * 
 * const resp = await fetch("http://example.com")
 * 
 * // cache for an hour
 * await responseCache.set("example-com", resp, 3600)
 * 
 * const cachedResponse = await responseCache.get("example-com")
 * ```
 * 
 * See {@link fly/cache} for caching lower level types.
 * @preferred
 * @module fly/cache/response
 */

/** */
import * as cache from "./cache"
import { Response } from '../dom_types'
import { FlyResponse } from "../response"

export const {
  del,
  expire,
  setTags
} = cache;

/**
 * Response metadata suitable for caching
 */
export interface Metadata {
  status: number,
  headers: { [key: string]: string | null },
  at?: number,
  ttl: number,
  tags?: string[]
}

export interface ResponseCacheSetOptions extends cache.CacheSetOptions {
  skipCacheHeaders?: string[]
}

/**
 * A response with cache info attached
 */
export type CachedResponse = Response & {
  key: string
}

/**
 * Get a Response object from cache.
 * @param key cache key to get
 * @return The response associated with the key, or null if empty
 */
export async function get(key: string): Promise<CachedResponse> {
  return cache.getEntry(key).then(entry => {
    if (!entry.meta || !entry.stream) {
      return null
    }
    try {
      const meta = JSON.parse(entry.meta);
      let age = 0;
      if (meta.at) {
        age = Math.round(Date.now() / 1000) - meta.at;
        meta.headers.Age = age.toString();
        meta.headers['Fly-Age'] = meta.headers.Age;
        delete meta.at;
      }
      const resp = new FlyResponse(entry.stream, meta)
      return Object.assign(resp, { key: key });
    } catch (e) {
      console.error("error getting response cache:", e);
      return null
    }
  })
}

const defaultSkipHeaders = [
  'authorization',
  'set-cookie'
];

/**
 * Stores a Response object in the Fly cache.
 * @param key Cache key to set
 * @param resp The response to cache
 * @param options Time to live
 */
export async function set(key: string, resp: Response, options?: ResponseCacheSetOptions | number) {
  const ttl = typeof options === "number" ? options : (options && options.ttl);
  let tags: string[] | undefined = undefined;
  let skipHeaderOption: string[] = defaultSkipHeaders;

  if (typeof options === 'number') {
    options = { ttl: options };
  } else if (typeof options === "object") {
    tags = options.tags;
    skipHeaderOption = [...skipHeaderOption, ...(options.skipCacheHeaders || []).map((headerKey) => headerKey.toLowerCase())];
  } else {
    options = {}
  }

  const meta = {
    status: resp.status,
    headers: {},
    at: Math.round(Date.now() / 1000),
    ttl: ttl,
    tags: tags
  }

  const body = await resp.clone().arrayBuffer();

  let etag = resp.headers.get("etag")
  if (!etag || etag == '') {
    etag = hex(await crypto.subtle.digest("SHA-1", body))
    resp.headers.set("etag", etag)
  }

  const skipHeaderSet = new Set(skipHeaderOption);
  for (const headerSet of resp.headers as any) {
    const [name, value] = headerSet;
    if (skipHeaderSet.has(name.toLowerCase())) {
      continue;
    }

    const existingVal = meta.headers[name];
    if (existingVal) {
      meta.headers[name] = `${existingVal}, ${value}`;
    } else {
      meta.headers[name] = value;
    }
  }

  return cache.set(key, body, Object.assign(options, { meta: JSON.stringify(meta) }))
}

/**
 * Resets the "age" of the cached Response object
 * @param key Response to "touch"
 */
export async function touch(key: string) {
  let entry = await cache.getEntry(key)
  if (!entry.meta) return false
  const meta = JSON.parse(entry.meta)
  meta.at = Math.round(Date.now() / 1000)
  return await cache.setMeta(key, JSON.stringify(meta))
}

// converts a buffer to hex, mainly for hashes
function hex(buffer: ArrayBuffer) {
  let hexCodes = [];
  let view = new DataView(buffer);
  for (let i = 0; i < view.byteLength; i += 4) {
    // Using getUint32 reduces the number of iterations needed (we process 4 bytes each time)
    let value = view.getUint32(i)
    // toString(16) will give the hex representation of the number without padding
    let stringValue = value.toString(16)
    // We use concatenation and slice for padding
    let padding = '00000000'
    let paddedValue = (padding + stringValue).slice(-padding.length)
    hexCodes.push(paddedValue);
  }

  // Join all the hex strings into one
  return hexCodes.join("");
}