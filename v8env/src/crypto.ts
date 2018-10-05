import { assert } from "./util";
import * as util from "./util";
import * as fbs from "./msg_generated";
import { flatbuffers } from "flatbuffers";
import { sendSync, sendAsync } from "./bridge";

/**
 * @private
 * @module fly
 * @hidden
 */

/** @hidden */

export const crypto = {
  subtle: {
    digest(algo: string, data: ArrayBufferView | ArrayBuffer): Promise<ArrayBufferLike> {
      const fbb = new flatbuffers.Builder();
      let algoidx = fbb.createString(algo);
      fbs.CryptoDigest.startCryptoDigest(fbb);
      fbs.CryptoDigest.addAlgo(fbb, algoidx);
      if (data instanceof ArrayBuffer)
        data = new DataView(data)
      return sendAsync(fbb, fbs.Any.CryptoDigest, fbs.CryptoDigest.endCryptoDigest(fbb), data).then(function (baseRes) {
        const msg = new fbs.CryptoDigestReady();
        baseRes.msg(msg);
        let u8 = msg.bufferArray();
        return u8.buffer.slice(u8.byteOffset);
      })
    },
  },
  getRandomValues(typedArray: Uint8Array): void {
    if (!(typedArray instanceof Uint8Array)) {
      throw new Error("Only Uint8Array are supported at present")
    }
    const fbb = new flatbuffers.Builder();
    fbs.CryptoRandomValues.startCryptoRandomValues(fbb);
    fbs.CryptoRandomValues.addLen(fbb, typedArray.length);
    const baseRes = sendSync(fbb, fbs.Any.CryptoRandomValues, fbs.CryptoRandomValues.endCryptoRandomValues(fbb));
    // const newArr = new Uint8Array(bridge.dispatchSync("getRandomValues", typedArray.length))
    const msg = new fbs.CryptoRandomValuesReady();
    baseRes.msg(msg);
    typedArray.set(msg.bufferArray());
    return
  }
}