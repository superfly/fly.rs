/**
 * Persistent, global key/value data store. Open collections, write data with `put`. Then retrieve data with `get`.
 * 
 * Keys and values are stored in range chunks. Chunks migrate to the region they're most frequently accessed from.
 * @module fly/data
 */

import { assert } from "../util";
import * as util from "../util";
import * as fbs from "../msg_generated";
import { flatbuffers } from "flatbuffers";
import { sendSync, sendAsync } from "../bridge";

/**
 * A collection of keys and values.
 */
export class Collection {
  name: string

  /**
   * Opens a collection
   * @param name name of the collection to open
   */
  constructor(name: string) {
    this.name = name
  }

  /**
   * Stores data in the collection associated key
   * @param key key for data
   * @param obj value to store
   */
  put(key: string, obj: string): Promise<boolean> {
    const fbb = new flatbuffers.Builder();
    const fbbColl = fbb.createString(this.name);
    const fbbKey = fbb.createString(key);
    const fbbObj = fbb.createString(JSON.stringify(obj));
    fbs.DataPut.startDataPut(fbb);
    fbs.DataPut.addCollection(fbb, fbbColl);
    fbs.DataPut.addKey(fbb, fbbKey);
    fbs.DataPut.addJson(fbb, fbbObj);
    return sendAsync(fbb, fbs.Any.DataPut, fbs.DataPut.endDataPut(fbb)).then(_baseRes => {
      return true
    })
  }

  /**
   * Retrieves data from the collection store
   * @param key key to retrieve
   */
  get(key: string): Promise<any> {
    const fbb = new flatbuffers.Builder();
    const fbbColl = fbb.createString(this.name);
    const fbbKey = fbb.createString(key);
    fbs.DataGet.startDataGet(fbb);
    fbs.DataGet.addCollection(fbb, fbbColl);
    fbs.DataGet.addKey(fbb, fbbKey);
    return sendAsync(fbb, fbs.Any.DataGet, fbs.DataGet.endDataGet(fbb)).then(baseRes => {
      if (baseRes.msgType() == fbs.Any.NONE)
        return null
      const msg = new fbs.DataGetReady();
      baseRes.msg(msg);
      return JSON.parse(msg.json())
    })
  }

  /**
   * Deletes data from the collection store.
   * @param key key to delete
   */
  del(key: string): Promise<boolean> {
    const fbb = new flatbuffers.Builder();
    const fbbColl = fbb.createString(this.name);
    const fbbKey = fbb.createString(key);
    fbs.DataDel.startDataDel(fbb);
    fbs.DataDel.addCollection(fbb, fbbColl);
    fbs.DataDel.addKey(fbb, fbbKey);
    return sendAsync(fbb, fbs.Any.DataDel, fbs.DataDel.endDataDel(fbb)).then(_baseRes => {
      return true
    })
  }
}

const data = {
  collection(name: string) {
    return new Collection(name)
  },
  dropCollection(name: string): Promise<boolean> {
    const fbb = new flatbuffers.Builder();
    const fbbColl = fbb.createString(this.name);
    fbs.DataDropCollection.startDataDropCollection(fbb);
    fbs.DataDropCollection.addCollection(fbb, fbbColl);
    return sendAsync(fbb, fbs.Any.DataDropCollection, fbs.DataDropCollection.endDataDropCollection(fbb)).then(_baseRes => {
      return true
    })
  }
}

export default data;