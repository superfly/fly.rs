import { stringifyTypeName } from "../util/format";
import { filterStackTrace } from "../source_maps";
import { isError } from "../util";
import { expect } from "chai/lib/chai.js";
import { exit } from "../os";
import { Compiler } from "./compiler";

export type DoneFn = (err?: any) => void;
export type TestFn = (done?: DoneFn) => void | Promise<void>;

export type ScopeFn = () => void;

type HookFn = TestFn;

interface TestDefinition {
  name: string;
  fn: TestFn;
  skip?: boolean;
  only?: boolean;
  parent: GroupDefinition;
}

interface GroupDefinition {
  name: string;
  parent?: GroupDefinition;
  groups: GroupDefinition[];
  tests: TestDefinition[];
  beforeAll: HookFn[];
  afterAll: HookFn[];
  beforeEach: HookFn[];
  afterEach: HookFn[];
}

export function test(name: string, fn: TestFn) {
  currentGroup().tests.push({ name, fn, parent: currentGroup() });
}

test.skip = (name: string, fn: TestFn) => {
  currentGroup().tests.push({ name, fn, skip: true, parent: currentGroup() });
}

test.only = (name: string, fn: TestFn) => {
  currentGroup().tests.push({ name, fn, only: true, parent: currentGroup() });
}

function beforeAll(fn: HookFn) {
  currentGroup().beforeAll.push(fn);
}

function beforeEach(fn: HookFn) {
  currentGroup().beforeEach.push(fn);
}

function afterEach(fn: HookFn) {
  currentGroup().afterEach.push(fn);
}

function afterAll(fn: HookFn) {
  currentGroup().afterAll.push(fn);
}

export function describe(name: string, scopeFn: ScopeFn) {
  const group = makeGroup(name);
  group.parent = currentGroup();
  pushGroup(group);
  scopeFn();
  popGroup();
  currentGroup().groups.push(group);
}

export const globals = {
  describe,
  test,
  it: test,
  beforeAll,
  beforeEach,
  afterAll,
  afterEach,
  before: beforeAll,
  after: afterAll,
  expect,
};

export function loadSuite(suitePath: string) {
  beginSuite(suitePath);
}

export async function run() {
  const runner = new Runner(suites);

  await runner.run();

  const { passed, failed, skipped } = runner.stats;

  printBlankLines(2);

  print(2, color(Style.green, `${passed} passing`));
  if (failed > 0) {
    print(2, color(Style.red, `${failed} failing`));
  }
  if (skipped > 0) {
    print(2, color(Style.yellow, `${skipped} skipped`));
  }

  printBlankLines(2);

  printFailures(runner.failures);

  if (failed > 0) {
    exit(1);
  } 
}

interface TestFailure {
  index: number;
  test: TestDefinition;
  error: Error;
}

export class Runner {
  private passed = 0;
  private failed = 0;
  private skipped = 0;

  public readonly failures: TestFailure[] = [];

  constructor(public suites: GroupDefinition[]) { }

  public get stats() {
    return {
      passed: this.passed,
      failed: this.failed,
      skipped: this.skipped,
    };
  }

  public async run() {
    for (const suite of this.suites) {
      await this.runGroup(suite);
    }
  }

  async runGroup(group: GroupDefinition) {
    const depth = path(group).length;
    print(depth, color(Style.groupName, group.name));

    for (const hook of group.beforeAll) {
      await runFn(hook);
    }
    for (const test of group.tests) {
      for (const hook of group.beforeEach) {
        await runFn(hook);
      }

      await this.runTest(test);

      for (const hook of group.afterEach) {
        await runFn(hook);
      }
    }
    for (const test of group.groups) {
      for (const hook of group.beforeEach) {
        await runFn(hook);
      }

      await this.runGroup(test);

      for (const hook of group.afterEach) {
        await runFn(hook);
      }
    }
    for (const hook of group.afterAll) {
      await runFn(hook);
    }
  }

  async runTest(test: TestDefinition) {
    const depth = path(test).length;

    if (test.skip) {
      this.skipped++;
      print(depth, `${color(Style.yellow, "○")} ${color(Style.dim, test.name)}`);
      return;
    }

    try {
      const timeout = new Promise((resolve, reject) => {
        setTimeout(() => {
          reject(new TestTimeoutError(test));
        }, 5000)
      });

      await Promise.race([
        runFn(test.fn),
        timeout
      ]);

      this.passed++;
      print(depth, `${color(Style.green, "✓")} ${color(Style.dim, test.name)}`);
    } catch (e) {
      this.failed++;
      const failure = this.recordFailure(test, e);
      const msg = `${failure.index}) ${test.name}`;
      print(depth, color(Style.red, msg));
    }
  }

  async runHook(fn: HookFn) {
    await runFn(fn);
  }

  recordFailure(test: TestDefinition, error: unknown) {
    const index = this.failures.length + 1;

    const failure = {
      index,
      test,
      error: isError(error) ? error : thrown2Error(error),
    };

    this.failures.push(failure);
    return failure;
  }
}

function print(depth: number, msg: string) {
  (window as any).logger.print("  ".repeat(depth) + msg);
}

let suites: GroupDefinition[] = [];

let root = makeGroup("")
let groupStack = [root];

function beginSuite(name: string) {
  const suiteRoot = makeGroup(name);
  suites.push(suiteRoot);
  groupStack = [suiteRoot];
}

// function endSuite() {
//   groupStack = [];
// }

function pushGroup(group: GroupDefinition) {
  groupStack.push(group);
}

function popGroup() {
  if (groupStack.length === 1) {
    throw new Error("cannot pop root group");
  }
  groupStack.pop();
}

function currentGroup() {
  return groupStack[groupStack.length - 1];
}

function makeGroup(name: string): GroupDefinition {
  return {
    name,
    tests: [],
    groups: [],
    beforeAll: [],
    afterAll: [],
    beforeEach: [],
    afterEach: [],
  };
}

function runFn(fn: TestFn): Promise<void> {
  return new Promise((resolve, reject) => {
    try {
      if (fn.length === 0) {
        let result = fn();
        // test returned a promise, resolve or reject from that
        if (result && typeof result.then === "function") {
          result.then(resolve, reject);
        } else {
          // test did not return a promise, mark done
          resolve();
        }
      } else if (fn.length === 1) {

        const done = (err?: unknown) => {
          if (err) {
            reject(err);
          } else {
            resolve();
          }
        }

        // test expects a done callback
        fn(done);
      } else {
        reject(new Error("Test functions only accept an optonal done callback"));
      };
    } catch (err) {
      reject(err);
    }
  });
}

const enum Style {
  pass = 90,
  fail = 31,

  yellow = 33,

  groupName = 0,
  red = 31,
  green = 32,
  dim = 90,
}

function color(style: Style, msg: string) {
  return '\x1b[' + style + 'm' + msg + '\x1b[0m';
}

function printFailures(failures: TestFailure[]) {
  for (const failure of failures) {
    print(2, color(Style.red, `${failure.index}) ${failure.test.name}`));

    printError(failure.error, 2)

    print(0, "");
  }
}

export function printSuiteError(suitePath: string, error: Error) {
  print(2, color(Style.red, `Error loading suite ${suitePath}`));

  printError(error, 3);
}

export function printError(error: Error, depth: number = 0) {
  if (error.stack) {
    const filteredStackTrace = filterStackTrace(error.stack);
    if (filteredStackTrace) {
      print(depth, color(Style.dim, filteredStackTrace));
    }
  } else if (error.message) {
    print(depth, color(Style.dim, error.message));
  } else {
    print(depth, color(Style.dim, error.toString()));
  }
}

function thrown2Error(err: any) {
  return new Error(
    `the ${stringifyTypeName(err)} ${JSON.stringify(err)} was thronw, throw an Error :)`
  );
}

function printBlankLines(count = 1) {
  print(0, "\n".repeat(count - 1));
}

function path(testOrGroup: TestDefinition | GroupDefinition): Array<TestDefinition | GroupDefinition> {
  if (testOrGroup.parent) {
    return [...path(testOrGroup.parent), testOrGroup];
  }
  return [testOrGroup];
}

export class TestTimeoutError extends Error {
  constructor(test: TestDefinition) {
    if (test.fn.length === 1) {
      super("Test timed out. Did you forget to call the `done` callback?");
    } else {
      super("Test timed out.");
    }
  }
}
