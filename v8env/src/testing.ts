
declare var console: any

export function assertEqual(expected: any, actual: any, msg?: string): void {
  // need better equality
  if (expected === actual) {
    return
  }
  if (!msg) {
    msg = `expected ${expected}, got ${actual}`
  }
  throw new Error(msg)
}

export type TestFn = () => void | Promise<void>

interface TestDefinition { 
  name: string,
  fn: TestFn
}

const tests: TestDefinition[] = []

export function test(name: string, fn: TestFn): void {
  tests.push({name, fn})  
}


export async function run() {
  let passed = 0
  let failed = 0

  console.log("Running Tests...")

  for (const test of tests) {
    const { name, fn } = test
    console.log(` - ${name}`)
    try {
      await fn()
      console.log("💚")
      passed++
    } catch (e) {
      console.log("💔")
      console.error((e && e.stack) || e);
      failed++
    }
  }

  const result = failed > 0 ? "💔" : "💚"
  console.log(`test results: ${result}. pass: ${passed} failed: ${failed}`)
}
