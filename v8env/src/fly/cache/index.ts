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

import { sendAsync, streams, sendStreamChunks, sendStreamChunk } from '../../bridge'
import * as fbs from "../../msg_generated";
import * as flatbuffers from "../../flatbuffers";
import { ReadableStream as WhatWGReadableStream } from '@stardazed/streams';
import { bufferFromStream } from '../../body_mixin';
import { ReadableStream } from '../../dom_types';
import { isIterable } from '../../util';
import * as global from "./global";

export { global };

export interface CacheSetOptions {
  ttl?: number;
  tags?: string[];
  meta?: string;
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

export interface CacheEntry {
  stream: ReadableStream | null,
  meta: string | null,
}

export function getEntry(key: string): Promise<CacheEntry> {
  return _get(key).then(raw => { return { stream: raw[0], meta: raw[1] } })
}

export function getStream(key: string): Promise<ReadableStream | null> {
  return _get(key).then(raw => raw[0])
}

function _get(key: string): Promise<[ReadableStream | null, string | null]> {
  const fbb = flatbuffers.createBuilder()
  const keyFbs = fbb.createString(key);
  fbs.CacheGet.startCacheGet(fbb);
  fbs.CacheGet.addKey(fbb, keyFbs);
  return sendAsync(fbb, fbs.Any.CacheGet, fbs.CacheGet.endCacheGet(fbb)).then(baseMsg => {
    const msg = new fbs.CacheGetReady();
    baseMsg.msg(msg);
    const stream = msg.stream() ?
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
      }) : null;
    return <[ReadableStream | null, string | null]>[stream, msg.meta()]
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
export function getMulti(keys: string[] | IterableIterator<string>): Promise<Array<ArrayBuffer | null>> {
  if (isIterable(keys)) {
    keys = Array.from(keys);
  }
  return Promise.all(keys.map(get));
}

/**
 * Get multiple string values from the cache
 * @param keys list of keys to retrieve
 * @returns list of results in the same order as the provided keys
 */
export async function getMultiString(keys: string[]): Promise<Array<string | null>> {
  return Promise.all(keys.map(getString));
}

/**
 * Sets a value at the specified key, with an optional ttl
 * @param key The key to add or overwrite
 * @param value Data to store at the specified key, up to 2MB
 * @returns true if the set was successful
 */
export function set(key: string, value: string | ArrayBuffer | ArrayBufferView | WhatWGReadableStream, options?: CacheSetOptions): Promise<boolean> {
  // TODO: validate value input

  const fbb = flatbuffers.createBuilder()
  const keyFbb = fbb.createString(key)

  let tags: number;
  let meta: number;

  if (options != null && typeof options === 'object') {
    if (Array.isArray(options.tags))
      tags = fbs.CacheSet.createTagsVector(fbb, options.tags.map(t => fbb.createString(t)));
    if (typeof options.meta === 'string')
      meta = fbb.createString(options.meta);
  }

  fbs.CacheSet.startCacheSet(fbb);
  fbs.CacheSet.addKey(fbb, keyFbb);
  if (typeof options === 'object' && typeof options.ttl === 'number') {
    fbs.CacheSet.addTtl(fbb, options.ttl)
  }
  if (typeof meta !== 'undefined') {
    fbs.CacheSet.addMeta(fbb, meta);
  }
  fbs.CacheSet.addTags(fbb, tags);

  return sendAsync(fbb, fbs.Any.CacheSet, fbs.CacheSet.endCacheSet(fbb)).then(async baseMsg => {
    let msg = new fbs.CacheSetReady()
    baseMsg.msg(msg);
    let id = msg.id()
    if (value instanceof WhatWGReadableStream) {
      await sendStreamChunks(id, value)
    } else {
      let buf: ArrayBufferView;
      if (typeof value === "string") {
        buf = new TextEncoder().encode(value)
      } else if (value instanceof ArrayBuffer) {
        buf = new Uint8Array(value)
      } else {
        buf = value
      }
      sendStreamChunk(id, true, buf);
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
export function expire(key: string, ttl: number) {
  const fbb = flatbuffers.createBuilder()
  const keyFbb = fbb.createString(key)
  fbs.CacheExpire.startCacheExpire(fbb);
  fbs.CacheExpire.addKey(fbb, keyFbb);
  fbs.CacheExpire.addTtl(fbb, ttl);

  return sendAsync(fbb, fbs.Any.CacheExpire, fbs.CacheExpire.endCacheExpire(fbb)).then(baseMsg => {
    return true
  })
}

export function setMeta(key: string, meta: string) {
  const fbb = flatbuffers.createBuilder()
  const keyFbb = fbb.createString(key)
  const metaFbb = fbb.createString(meta)
  fbs.CacheSetMeta.startCacheSetMeta(fbb);
  fbs.CacheSetMeta.addKey(fbb, keyFbb);
  fbs.CacheSetMeta.addMeta(fbb, metaFbb);

  return sendAsync(fbb, fbs.Any.CacheSetMeta, fbs.CacheSetMeta.endCacheSetMeta(fbb)).then(baseMsg => {
    return true
  })
}

/**
 * Replace tags for a given cache key
 * @param key The key to modify
 * @param tags Tags to apply to key
 * @returns true if tags were successfully updated
 */
export function setTags(key: string, tags: string[]) {
  const fbb = flatbuffers.createBuilder()
  const keyFbb = fbb.createString(key)

  const tagsFbb = fbs.CacheSet.createTagsVector(fbb, tags.map(t => fbb.createString(t)));

  fbs.CacheSetTags.startCacheSetTags(fbb);
  fbs.CacheSetTags.addKey(fbb, keyFbb);
  fbs.CacheSetTags.addTags(fbb, tagsFbb);

  return sendAsync(fbb, fbs.Any.CacheSetTags, fbs.CacheSetTags.endCacheSetTags(fbb)).then(_baseMsg => {
    return true
  })
}

/**
 * Purges all cache entries with the given tag
 * @param tag Tag to purge
 */
export function purgeTag(tag: string) {
  const fbb = flatbuffers.createBuilder()
  const tagFbb = fbb.createString(tag)

  fbs.CachePurgeTag.startCachePurgeTag(fbb);
  fbs.CachePurgeTag.addTag(fbb, tagFbb);

  return sendAsync(fbb, fbs.Any.CachePurgeTag, fbs.CachePurgeTag.endCachePurgeTag(fbb)).then(_baseMsg => {
    return true
  })
}


/**
 * Deletes the value (if any) at the specified key
 * @param key Key to delete
 * @returns true if delete was successful
 */
export function del(key: string) {
  const fbb = flatbuffers.createBuilder()
  const keyFbb = fbb.createString(key)
  fbs.CacheDel.startCacheDel(fbb);
  fbs.CacheDel.addKey(fbb, keyFbb);

  return sendAsync(fbb, fbs.Any.CacheDel, fbs.CacheDel.endCacheDel(fbb)).then(baseMsg => {
    return true
  })
}
