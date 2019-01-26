/**
 * The Fly global cache API, allows eventually consistent modifications to all caches in all regions.
 * 
 * ```javascript
 * import cache from "@fly/cache"
 * 
 * // notify all caches to delete a key
 * await cache.global.del("key-to-delete")
 * 
 * // notify all caches to purge a tag
 * await cache.global.purgeTag("key-to-purge")
 * ```
 * 
 * @module fly/cache/global
 */

import { sendAsync } from '../../bridge'
import * as fbs from "../../msg_generated";
import * as flatbuffers from "../../flatbuffers";

/**
 * Notifies all caches to delete data at the specified key.
 * @param key the key to delete
 * @returns A promise that resolves as soon as the del notification is sent. Since regional caches are
 *  eventually consisten, this may return before every cache is updated.
 */
export async function del(key: string): Promise<boolean> {
  const fbb = flatbuffers.createBuilder()
  const keyFbb = fbb.createString(key)
  fbs.CacheNotifyDel.startCacheNotifyDel(fbb);
  fbs.CacheNotifyDel.addKey(fbb, keyFbb);
  return sendAsync(fbb, fbs.Any.CacheNotifyDel, fbs.CacheNotifyDel.endCacheNotifyDel(fbb)).then(_baseMsg => {
    return true
  })
}

/**
 * Notifies all regional caches to purge keys with the specified tag.
 * @param tag the tag to purge
 * @returns A promise that resolves as soon as the purge notification is sent. Since regional caches are
 *  eventually consisten, this may return before every cache is updated.
 */
export async function purgeTag(tag: string): Promise<boolean> {
  const fbb = flatbuffers.createBuilder()
  const tagFbb = fbb.createString(tag)
  fbs.CacheNotifyPurgeTag.startCacheNotifyPurgeTag(fbb);
  fbs.CacheNotifyPurgeTag.addTag(fbb, tagFbb);
  return sendAsync(fbb, fbs.Any.CacheNotifyPurgeTag, fbs.CacheNotifyPurgeTag.endCacheNotifyPurgeTag(fbb)).then(_baseMsg => {
    return true
  })
}
