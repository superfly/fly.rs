/**
 * Persistent, global key/value data store. Open collections, write data with `put`. Then retrieve data with `get`.
 * 
 * Keys and values are stored in range chunks. Chunks migrate to the region they're most frequently accessed from.
 * @module fly/data
 */

import * as fbs from "../msg_generated";
import * as flatbuffers from "../flatbuffers";
import { sendAsync } from "../bridge";

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
  async put(key: string, obj: any): Promise<boolean> {
    if (typeof obj === "number" || obj === undefined || obj === null) {
      throw new TypeError("value must be a string, object, or array");
    }

    const fbb = flatbuffers.createBuilder();
    const fbbColl = fbb.createString(this.name);
    const fbbKey = fbb.createString(key);
    const fbbObj = fbb.createString(JSON.stringify(obj));
    fbs.DataPut.startDataPut(fbb);
    fbs.DataPut.addCollection(fbb, fbbColl);
    fbs.DataPut.addKey(fbb, fbbKey);
    fbs.DataPut.addJson(fbb, fbbObj);

    await sendAsync(fbb, fbs.Any.DataPut, fbs.DataPut.endDataPut(fbb));

    return true;
  }

  /**
   * Retrieves data from the collection store
   * @param key key to retrieve
   */
  get(key: string): Promise<any> {
    const fbb = flatbuffers.createBuilder();
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
    const fbb = flatbuffers.createBuilder();
    const fbbColl = fbb.createString(this.name);
    const fbbKey = fbb.createString(key);
    fbs.DataDel.startDataDel(fbb);
    fbs.DataDel.addCollection(fbb, fbbColl);
    fbs.DataDel.addKey(fbb, fbbKey);
    return sendAsync(fbb, fbs.Any.DataDel, fbs.DataDel.endDataDel(fbb)).then(_baseRes => {
      return true
    })
  }

  increment(key: string, field: string, amount?: number): Promise<boolean> {
    const fbb = flatbuffers.createBuilder();
    const fbbColl = fbb.createString(this.name);
    const fbbKey = fbb.createString(key);
    const fbbField = fbb.createString(field);
    fbs.DataIncr.startDataIncr(fbb);
    fbs.DataIncr.addCollection(fbb, fbbColl);
    fbs.DataIncr.addKey(fbb, fbbKey);
    fbs.DataIncr.addField(fbb, fbbField);
    fbs.DataIncr.addAmount(fbb, amount || 1);
    return sendAsync(fbb, fbs.Any.DataIncr, fbs.DataIncr.endDataIncr(fbb)).then(_baseRes => {
      return true
    })
  }
}

export function collection(name: string) {
  return new Collection(name)
}

export function dropCollection(name: string): Promise<boolean> {
  const fbb = flatbuffers.createBuilder();
  const fbbColl = fbb.createString(name);
  fbs.DataDropCollection.startDataDropCollection(fbb);
  fbs.DataDropCollection.addCollection(fbb, fbbColl);
  return sendAsync(fbb, fbs.Any.DataDropCollection, fbs.DataDropCollection.endDataDropCollection(fbb)).then(_baseRes => {
    return true
  })
}

function assertValueType(val: any) {
  // if (val === )
}
