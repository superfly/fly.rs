import { stringifyTypeName } from "../util/format";
import { filterStackTrace } from "../source_maps";
import { isError } from "../util";
import { expect } from "chai/lib/chai.js";
import { exit } from "../os";

export type DoneFn = (err?: any) => void;
export type RunnableFn = (done?: DoneFn) => Promise<void> | void;

export type ScopeFn = () => void;

const DefaultTimeout = 5000;

interface TestDefinition {
  name: string;
  fn: RunnableFn;
  skip?: boolean;
  only?: boolean;
  parent: GroupDefinition;
  timeout: number;
}

interface GroupDefinition {
  name: string;
  parent?: GroupDefinition;
  groups: GroupDefinition[];
  tests: TestDefinition[];
  beforeAll: RunnableFn[];
  afterAll: RunnableFn[];
  beforeEach: RunnableFn[];
  afterEach: RunnableFn[];
}

export function test(name: string, fn: RunnableFn, timeout: number = DefaultTimeout) {
  currentGroup().tests.push({ name, fn, parent: currentGroup(), timeout });
}

test.skip = (name: string, fn: RunnableFn, timeout: number = DefaultTimeout) => {
  currentGroup().tests.push({ name, fn, skip: true, parent: currentGroup(), timeout });
}

test.only = (name: string, fn: RunnableFn, timeout: number = DefaultTimeout) => {
  currentGroup().tests.push({ name, fn, only: true, parent: currentGroup(), timeout });
}

function beforeAll(fn: RunnableFn) {
  currentGroup().beforeAll.push(fn);
}

function beforeEach(fn: RunnableFn) {
  currentGroup().beforeEach.push(fn);
}

function afterEach(fn: RunnableFn) {
  currentGroup().afterEach.push(fn);
}

function afterAll(fn: RunnableFn) {
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

  try {
    await runner.run();
  } catch (error) {
    printBlankLines(2);
    printError(error, 2);

    exit(1);
  }

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
      await this.runHook(hook)
        .catch(error => {
          print(depth, color(Style.red, "Error running beforeAll hook"));
          throw error;
        });
    }
  
    for (const test of group.tests) {
      for (const hook of group.beforeEach) {
        await this.runHook(hook)
          .catch(error => {
            print(depth, color(Style.red, "Error running beforeEach hook"));
            throw error;
          });
      }

      await this.runTest(test);

      for (const hook of group.afterEach) {
        await this.runHook(hook)
          .catch(error => {
            print(depth, color(Style.red, "Error running afterEach hook"));
            throw error;
          });
      }
    }
  
    for (const test of group.groups) {
      for (const hook of group.beforeEach) {
        await this.runHook(hook)
          .catch(error => {
            print(depth, color(Style.red, "Error running beforeEach hook"));
            throw error;
          });
      }

      await this.runGroup(test);

      for (const hook of group.afterEach) {
        await this.runHook(hook)
          .catch(error => {
            print(depth, color(Style.red, "Error running afterEach hook"));
            throw error;
          });
      }
    }
    for (const hook of group.afterAll) {
      await this.runHook(hook)
        .catch(error => {
          print(depth, color(Style.red, "Error running afterAll hook"));
          throw error;
        });
    }
  }

  runHook(fn: RunnableFn): Promise<void> {
    return callFn(fn, DefaultTimeout);
  }

  async runTest(test: TestDefinition): Promise<void> {
    const depth = path(test).length;

    if (test.skip) {
      this.skipped++;
      print(depth, `${color(Style.yellow, "○")} ${color(Style.dim, test.name)}`);
      return Promise.resolve();
    }

    return callFn(test.fn, test.timeout)
      .then(() => {
        this.passed++;
        print(depth, `${color(Style.green, "✓")} ${color(Style.dim, test.name)}`);
      })
      .catch(error => {
        this.failed++;
        const failure = this.recordFailure(test, error);
        const msg = `${failure.index}) ${test.name}`;
        print(depth, color(Style.red, msg));
      });
  }

  recordFailure(test: TestDefinition, error: unknown) {
    const index = this.failures.length + 1;

    const failure = {
      index,
      test,
      error: isError(error) ? error : normalizeReason(error),
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

function callFn(fn: RunnableFn, timeout: number ): Promise<void> {
  let timeoutId: number; 
  return new Promise((resolve, reject) => {
    timeoutId = setTimeout(
      () => reject(new TestTimeoutError(fn)),
      timeout
    );

    // fn expects a done callback
    if (fn.length) {
      const done = (reason?: Error | string) => {
        return reason ? reject(reason) : resolve();
      }

      return fn(done);
    }

    let returnVal: any;
    try {
      returnVal = fn();
    } catch (error) {
      return reject(error);
    }

    // if fn returns a promise, return it
    if (typeof returnVal === "object" && returnVal !== null && typeof returnVal.then === "function") {
      return returnVal.then(resolve, reject);
    }

    // test is a synchronous function, and if we got here it passed
    return resolve();
  }).then(() => {
    clearTimeout(timeoutId);
  }).catch(error => {
    clearTimeout(timeoutId);
    throw error;
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

function normalizeReason(err: any) {
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

export class HookError extends Error {
  constructor() {
    super("Error running hook");
  }
}

export class TestTimeoutError extends Error {
  constructor(fn: RunnableFn) {
    if (fn.length) {
      super("Timeout. Make sure this test is calling the `done` callback!");
    } else {
      super("Timeout");
    }
  }
}
