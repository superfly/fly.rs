import { testStream } from "./fixtures/test-stream";

describe("fly.cache", () => {
  describe("set+get", () => {
    test("String->ArrayBuffer", async () => {
      const [key, value] = kv();

      const setResult = await fly.cache.set(key, value)
      expect(setResult).to.eq(true, "couldn't set test value")
      const result = await fly.cache.get(key)
      expect(result).to.be.a("ArrayBuffer")
      expect(ab2str(result)).to.eq(value)
    })

    test("ArrayBuffer->ArrayBuffer", async () => {
      const [key, value] = kv();
      
      const setResult = await fly.cache.set(key, value)
      expect(setResult).to.eq(true, "couldn't set test value")
      const result = await fly.cache.get(key)
      expect(result).to.be.a("ArrayBuffer")
      expect(ab2str(result)).to.eq(value)
    })

    test("Stream->ArrayBuffer", async () => {
      const [key, value] = kv("this-is-a-value-that-will-be-streamed");
      const stream = testStream(value);

      const setResult = await fly.cache.set(key, stream)
      expect(setResult).to.eq(true, "couldn't set test value")
      const result = await fly.cache.get(key)
      expect(result).to.be.a("ArrayBuffer")
      expect(ab2str(result)).to.eq(value)
    })

    test("Missing key", async () => {
      const [k, _] = kv();
      expect(await fly.cache.get(k)).to.be.null
    })

    test("Empty ArrayBuffer", async () => {
      const k = `cache-test${Math.random()}`

      await fly.cache.set(k, new ArrayBuffer(0))

      const result = await fly.cache.get(k)
      expect(result).to.be.a('ArrayBuffer')
      expect(result.byteLength).to.eq(0)
    })

    test("Empty string", async () => {
      const k = `cache-test${Math.random()}`
      await fly.cache.set(k, '')
      const result = await fly.cache.getString(k)
      expect(result).to.eq('')
    })
  })

  describe("getMulti()", () => {
    test("returns results in order", async () => {
      const entries = new Map([kv(), kv(), kv()]);

      for (const [k, v] of entries) {
        const result = await fly.cache.set(k, v)
        expect(result).to.eq(true, `couldn't set key`)
      }

      const results = await fly.cache.getMulti(Array.from(entries.keys()));
      expect(results).to.be.an.instanceOf(Array)
      expect(results.map(ab2str)).to.have.ordered.members(Array.from(entries.values()));
    })

    test("handles missing keys", async () => {
      const entries = new Map([kv(), kv(), kv()]);

      for (const [k, v] of entries) {
        const result = await fly.cache.set(k, v)
        expect(result).to.eq(true, `couldn't set key`)
      }

      entries.set("missing", null)

      const results = await fly.cache.getMulti(Array.from(entries.keys()));
      expect(results).to.be.an.instanceOf(Array)
      expect(results.map(r => r && ab2str(r))).to.have.ordered.members(Array.from(entries.values()));
    })
  })

  describe("getMultiString()", () => {
    test("returns results in order", async () => {
      const entries = new Map([kv(), kv(), kv()]);

      for (const [k, v] of entries) {
        const result = await fly.cache.set(k, v)
        expect(result).to.eq(true, `couldn't set key`)
      }

      const results = await fly.cache.getMultiString(Array.from(entries.keys()));
      expect(results).to.be.an.instanceOf(Array)
      expect(results).to.have.ordered.members(Array.from(entries.values()));
    })

    test("handles missing keys", async () => {
      const entries = new Map([kv(), kv(), kv()]);

      for (const [k, v] of entries) {
        const result = await fly.cache.set(k, v)
        expect(result).to.eq(true, `couldn't set key`)
      }

      entries.set("missing", null)

      const results = await fly.cache.getMultiString(Array.from(entries.keys()));
      expect(results).to.be.an.instanceOf(Array)
      expect(results).to.have.ordered.members(Array.from(entries.values()));
    })
  })

  test("getString()", async () => {
    const [key, value] = kv();

    const setResult = await fly.cache.set(key, value)
    expect(setResult).to.eq(true, "couldn't set test value")
    const result = await fly.cache.getString(key)
    expect(result).to.eq(value)
  })

  test("getStream()", async () => {
    const [key, value] = kv();

    const setResult = await fly.cache.set(key, value)
    expect(setResult).to.eq(true, "couldn't set test value")
    const result = await fly.cache.getStream(key)
    expect(result).to.be.an.instanceOf(ReadableStream)
    const buffer = await streamToBuffer(result.getReader())
    expect(ab2str(buffer)).to.eq(value)
  })
  
  test("del()", async () => {
    const [key, value] = kv();
    const setResult = await fly.cache.set(key, value)
    expect(setResult).to.eq(true)

    expect(
      await fly.cache.del(key)
    ).to.eq(true, "del should return true")

    let newVal = await fly.cache.get(key)
    expect(newVal).to.eq(null, "previously deleted key should be null")
  })

  describe("TTL", () => {
    test("set with ttl", async () => {
      const [key, value] = kv();

      const setResult = await fly.cache.set(key, value, { ttl: 1 })
      expect(setResult).to.eq(true)

      while (await fly.cache.getString(key)) {
        await new Promise(r => setTimeout(r, 50))
      }
    })

    test("expire", async () => {
      const [key, value] = kv();

      expect(
        await fly.cache.set(key, value)
      ).to.eq(true)

      expect(
        await fly.cache.expire(key, 1)
      ).to.eq(true)
      
      while (await fly.cache.getString(key)) {
        await new Promise(r => setTimeout(r, 50))
      }
    })
  })

  test.skip("handles set.onlyIfEmpty", async () => {
    const k = `cache-test${Math.random()}`
    await fly.cache.set(k, 'asdf')

    const setResult = await fly.cache.set(k, 'jklm', { onlyIfEmpty: true })
    const v = await fly.cache.getString(k)

    expect(setResult).to.eq(false)
    expect(v).to.eq("asdf")
  })
})

function kv(value = "value") {
  return [`k:${Math.random()}`, `v:${Math.random()}:${value}`];
}
