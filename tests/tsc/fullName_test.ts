import { fullName } from "./fullName"

describe("fullName", () => {
  it("it works", () => {
    const expected = 'a b'
    const actual = fullName("a", "b")
    if (expected !== actual) {
      throw new Error(`Expected ${expected}, got ${actual}`)
    }
  })
})