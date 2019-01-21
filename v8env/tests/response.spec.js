describe("Response", () => {
  it("errors on unknown body types", () => {
    expect(() => new Response(1)).to.throw(/Bad FlyResponse body type/)
    expect(() => new Response(true)).to.throw(/Bad FlyResponse body type/)
    expect(() => new Response({})).to.throw(/Bad FlyResponse body type/)
    expect(() => new Response(["wat"])).to.throw(/Bad FlyResponse body type/)
  })
})
describe("Response.redirect", () => {
  it("creates a 302 by default", () => {
    const r = Response.redirect("http://test.com")
    expect(r.status).to.eq(302)
    expect(r.headers.get("location")).to.eq("http://test.com")
  })

  it('allows a custom status', () => {
    const r = Response.redirect("http://test.com", 307)
    expect(r.status).to.eq(307)
  })
})