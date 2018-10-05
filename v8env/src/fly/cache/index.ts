/**
 * An API for accessing a regional, volatile cache. Data stored in `@fly/cache` can have an associated per-key time to live (TTL), and we will evict key data automatically after the elapsed TTL. We will also evict unused data when we need to reclaim space.
 * 
 * ```javascript
 * import cache from "@fly/cache"
 * 
 * await cache.set("test-key", "test-value")
 * 
 * const s = await cache.getString("test-key")
 * ```
 * 
 * See {@link fly/cache/response} for caching HTTP Response objects.
 * 
 * See {@link fly/cache/global} for global cache del/purge
 * 
 * @preferred
 * @module fly/cache
 */

/** */
import { sendSync, sendAsync, streams, sendStreamChunks, sendStreamChunk } from '../../bridge'
import * as fbs from "../../msg_generated";
import { flatbuffers } from "flatbuffers";

export interface CacheSetOptions {
  ttl?: number;
  tags?: string[];
  onlyIfEmpty?: boolean;
}

/**
 * Get an ArrayBuffer value (or null) from the cache
 * @param key The key to get
 * @return Raw bytes stored for provided key or null if empty.
 */
export function get(key: string): Promise<ArrayBufferLike | null> {
  return getStream(key).then(stream => {
    if (!stream)
      return null
    return bufferFromStream(stream.getReader())
  })
}

export function getStream(key: string): Promise<ReadableStream | null> {
  const fbb = new flatbuffers.Builder()
  const keyFbs = fbb.createString(key);
  fbs.CacheGet.startCacheGet(fbb);
  fbs.CacheGet.addKey(fbb, keyFbs);
  return sendAsync(fbb, fbs.Any.CacheGet, fbs.CacheGet.endCacheGet(fbb)).then(baseMsg => {
    const msg = new fbs.CacheGetReady();
    baseMsg.msg(msg);
    const id = msg.id()
    return msg.stream() ?
      new WhatWGReadableStream({
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
  })
}

/**
 * Get a string value (or null) from the cache
 *
 * @param key The key to get
 * @returns Data stored at the key, or null if none exists
 */
export function getString(key: string): Promise<string | null> {
  return get(key).then(buf => {
    if (!buf)
      return null
    return new TextDecoder("utf-8").decode(buf)
  })
}

/**
 * Get multiple values from the cache.
 * @param keys list of keys to retrieve
 * @returns List of results in the same order as the provided keys
 */
// export function getMulti(keys: string[]): Promise<(ArrayBuffer | null)[]> {
//   return new Promise<(ArrayBuffer | null)[]>(function cacheGetMultiPromise(resolve, reject) {
//     bridge.dispatch(
//       "flyCacheGetMulti",
//       JSON.stringify(keys),
//       function cacheGetMultiCallback(err: string | null | undefined, ...values: (ArrayBuffer | null)[]) {
//         if (err != null) {
//           reject(err)
//           return
//         }
//         resolve(values)
//       })
//   })
// }

/**
 * Get multiple string values from the cache
 * @param keys list of keys to retrieve
 * @returns list of results in the same order as the provided keys
 */
// export async function getMultiString(keys: string[]) {
//   const raw = await getMulti(keys)
//   return raw.map((b) => b ? new TextDecoder("utf-8").decode(b) : null)
// }

/**
 * Sets a value at the specified key, with an optional ttl
 * @param key The key to add or overwrite
 * @param value Data to store at the specified key, up to 2MB
 * @param ttl Time to live (in seconds)
 * @returns true if the set was successful
 */
export function set(key: string, value: string | ArrayBuffer | ArrayBufferView | WhatWGReadableStream, options?: CacheSetOptions | number): Promise<boolean> {
  // if (typeof value !== "string" && !(value instanceof ArrayBuffer)) {
  //   throw new Error("Cache values must be either a string or array buffer")
  // }

  const fbb = new flatbuffers.Builder()
  const keyFbb = fbb.createString(key)
  fbs.CacheSet.startCacheSet(fbb);
  fbs.CacheSet.addKey(fbb, keyFbb);
  return sendAsync(fbb, fbs.Any.CacheSet, fbs.CacheSet.endCacheSet(fbb)).then(async baseMsg => {
    console.log("got cache set ready!");
    let msg = new fbs.CacheSetReady()
    baseMsg.msg(msg);
    let id = msg.id()
    console.log("id:", id)
    if (value instanceof WhatWGReadableStream) {
      console.log("got a stream");
      await sendStreamChunks(id, value)
    } else {
      console.log("not a readable stream I guess!");
      let buf: ArrayBufferView;
      if (typeof value === "string") {
        console.log("string")
        buf = new TextEncoder().encode(value)
      } else if (value instanceof ArrayBuffer) {
        console.log("array buf")
        buf = new Uint8Array(value)
      } else {
        console.log("array buf view")
        buf = value
      }
      sendStreamChunk(id, true, buf);
      console.log("sent, done.");
    }
    return true
  })
}

/**
 * Add or overwrite a key's  time to live
 * @param key The key to modify
 * @param ttl Expiration time remaining in seconds
 * @returns true if ttl was successfully updated
 */
// export function expire(key: string, ttl: number) {
//   return new Promise<boolean>(function cacheSetPromise(resolve, reject) {
//     bridge.dispatch("flyCacheExpire", key, ttl, function cacheSetCallback(err: string | null, ok?: boolean) {
//       if (err != null) {
//         reject(err)
//         return
//       }
//       resolve(ok)
//     })
//   })
// }

/**
 * Replace tags for a given cache key
 * @param key The key to modify
 * @param tags Tags to apply to key
 * @returns true if tags were successfully updated
 */
// export function setTags(key: string, tags: string[]) {
//   return new Promise<boolean>(function cacheSetTagsPromise(resolve, reject) {
//     bridge.dispatch("flyCacheSetTags", key, tags, function cacheSetTagsCallback(err: string | null, ok?: boolean) {
//       if (err != null) {
//         reject(err)
//         return
//       }
//       resolve(ok)
//     })
//   })
// }

/**
 * Purges all cache entries with the given tag
 * @param tag Tag to purge
 */
// export function purgeTag(tag: string) {
//   return new Promise<string[]>(function cachePurgeTagsPromise(resolve, reject) {
//     bridge.dispatch("flyCachePurgeTags", tag, function cachePurgeTagsCallback(err: string | null, keys?: string) {
//       if (err != null || !keys) {
//         reject(err || "weird result")
//         return
//       }
//       const result = JSON.parse(keys)
//       if (result instanceof Array) {
//         resolve(<string[]>result)
//         return
//       } else {
//         reject("got back gibberish")
//       }
//     })
//   })
// }


/**
 * Deletes the value (if any) at the specified key
 * @param key Key to delete
 * @returns true if delete was successful
 */
// export function del(key: string) {
//   return new Promise<boolean>(function cacheDelPromise(resolve, reject) {
//     bridge.dispatch("flyCacheDel", key, function cacheDelCallback(err: string | null, ok?: boolean) {
//       if (err != null) {
//         reject(err)
//         return
//       }
//       resolve(ok)
//     })
//   })
// }

/**
 * A library for caching/retrieving Response objects
 * 
 * See {@link fly/cache/response}
 */
// export { default as responseCache } from "./response"

/**
 * API for sending global cache notifications
 * 
 * See {@link fly/cache/global} 
 */
// import { default as global } from "./global"
import { ReadableStream as WhatWGReadableStream } from '@stardazed/streams';
import { bufferFromStream } from '../../body_mixin';
import { ReadableStream } from '../../dom_types';

const cache = {
  get,
  getString,
  getStream,
  // getMulti,
  // getMultiString,
  set,
  // expire,
  // del,
  // setTags,
  // purgeTag,
  // global
}
export default cache