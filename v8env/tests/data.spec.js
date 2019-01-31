const db = fly.data;

describe('@fly/data', () => {
  describe("collection()", () => {
    test("returns a Collection", () => {
      expect(db.collection("testing")).to.be.instanceOf(fly.data.Collection)
    })
  })

  describe("Collection", () => {
    describe("put+get", () => {
      afterEach(async () => db.dropCollection("testing"))

      describe("accepts value type", () => {
        const cases = [
          ["Object", { name: "Michael" }],
          ["String", "Michael"],
          ["Array", ["Michael", "Dwan"]],
        ]

        for (const [name, val] of cases) {
          test(name, async () => {
            const coll = db.collection("testing")

            expect(
              await coll.put("key", val)
            ).to.be.true

            expect(
              await coll.get("key")
            ).to.eql(val)
          })
        }
      })

      describe("rejects value type", () => {
        const cases = [
          ["Number", 123],
          ["undefined", undefined],
          ["null", null],
        ]

        for (const [name, val] of cases) {
          test(name, async () => {
            const coll = db.collection("testing")

            try {
              await coll.put("key", val)
              expect.fail(`value type ${name} should be rejected`)
            } catch (err) {
              expect(err).to.be.an.instanceOf(TypeError)
            }
          })
        }
      })

      test("upserts data", async () => {
        const coll = db.collection("testing")

        // insert initial object
        let ok = await coll.put("yo", { some: "json" })
        expect(ok).to.be.true

        // replace the object
        ok = await coll.put("yo", { some: "json2" })
        expect(ok).to.be.true

        // get the obj, to assert its value
        const res = await coll.get("yo")
        expect(res).to.deep.equal({ some: "json2" })
      })

      test("get with missing key", async () => {
        const coll = db.collection("testing")

        let result = await coll.get("missing-key");
        expect(result).to.be.null;
      })
    })

    describe(".del", () => {
      it("delete data", async () => {
        const coll = db.collection("testing")
        const ok = await coll.put("yo", { some: "json" })
        expect(ok).to.be.true

        const okDel = await coll.del("yo")
        expect(okDel).to.equal(true)

        const res = await coll.get("yo")
        expect(res).to.equal(null)
      })
    })

    describe("increment", () => {
      test("works", async () => {
        const coll = db.collection("testing")

        expect(
          await coll.put("inc-test", { count: 1 })
        ).to.eq(true)

        expect(
          await coll.increment("inc-test", "count")
        ).to.be.true

        expect(
          await coll.get("inc-test")
        ).to.eql({ count: 2 })

        expect(
          await coll.increment("inc-test", "count", 5)
        ).to.be.true

        expect(
          await coll.get("inc-test")
        ).to.eql({ count: 7 })
      })

      test("with negative number", async () => {
        const coll = db.collection("testing")

        expect(
          await coll.put("inc-test", { count: 8 })
        ).to.eq(true)

        expect(
          await coll.increment("inc-test", "count", -5)
        ).to.be.true

        expect(
          await coll.get("inc-test")
        ).to.eql({ count: 3 })
      })


      test("preserves other keys", async () => {
        const coll = db.collection("testing")

        expect(
          await coll.put("inc-test", { another:"key", count: 1 })
        ).to.eq(true)

        expect(
          await coll.increment("inc-test", "count")
        ).to.be.true

        expect(
          await coll.get("inc-test")
        ).to.eql({ another: "key", count: 2 })
      })
    })
  })
})