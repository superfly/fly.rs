// const db = fly.data;
// const collection = fly.data.Collection;

// describe('@fly/data', () => {
//     describe('.collection', () => {
//         it("returns a Collection", () => {
//             expect(db.collection("testing")).to.be.instanceOf(Collection)
//         })
//     })

//     describe("Collection", () => {
//         describe(".put", () => {
//             it("upserts data", async () => {
//                 const coll = db.collection("testing")

//                 // insert initial object
//                 let ok = await coll.put("yo", { some: "json" })
//                 expect(ok).to.be.true

//                 // replace the object
//                 ok = await coll.put("yo", { some: "json2" })
//                 expect(ok).to.be.true

//                 // get the obj, to assert its value
//                 const res = await coll.get("yo")
//                 expect(res).to.deep.equal({ some: "json2" })
//             })
//             after(() => db.dropCollection("testing").catch(e => { console.log("COULD NOT DROP COLL:", e) }))
//         })

//         describe(".del", () => {
//             it("delete data", async () => {
//                 const coll = db.collection("testing")
//                 const ok = await coll.put("yo", { some: "json" })
//                 expect(ok).to.be.true

//                 const okDel = await coll.del("yo")
//                 expect(okDel).to.equal(true)

//                 const res = await coll.get("yo")
//                 expect(res).to.equal(null)
//             })
//         })
//     })
// })