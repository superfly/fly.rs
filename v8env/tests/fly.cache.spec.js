describe("fly.cache", () => {
    it("gets a string", async () => {
        const v = `cache-value-woo! ${Math.random()}`

        const setResult = await fly.cache.set("cache-test-key", v)
        expect(setResult).to.eq(true, "couldn't set test value")
        const result = await fly.cache.getString("cache-test-key")

        expect(result).to.eq(v)
    })

    it("deletes from cache", async () => {
        const v = `cache-value-woo! ${Math.random()}`

        const setResult = await fly.cache.set("cache-delete-key", v)
        expect(setResult).to.eq(true)

        const result = await fly.cache.del("cache-delete-key")

        expect(result).to.eq(true, "del should return true")

        let newVal = await fly.cache.get("cache-delete-key")
        expect(newVal).to.eq(null, "previously deleted key should be null")
    })

    it("accepts empty arrayBuffer", async () => {
        const k = `cache-test${Math.random()}`

        await fly.cache.set(k, new ArrayBuffer(0))

        const result = await fly.cache.get(k)
        expect(result).to.be.a('ArrayBuffer')
        expect(result.byteLength).to.eq(0)
    })

    it("handles blank strings", async () => {
        const k = `cache-test${Math.random()}`
        await fly.cache.set(k, '')
        const result = await fly.cache.getString(k)
        expect(result).to.eq('')
    })

    // it("handles set.onlyIfEmpty", async () => {
    //     const k = `cache-test${Math.random()}`
    //     await fly.cache.set(k, 'asdf')

    //     const setResult = await fly.cache.set(k, 'jklm', { onlyIfEmpty: true })
    //     const v = await fly.cache.getString(k)

    //     expect(setResult).to.eq(false)
    //     expect(v).to.eq("asdf")
    // })
})