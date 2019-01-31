import { Compiler } from "./compiler";
import { run, globals, loadSuite, printSuiteError } from "./testing";
import { DevTools, ConfigOptions } from "./api";
import { exit } from "../os";

class FlyDevTools implements DevTools {
  private compiler: Compiler;

  constructor(config: ConfigOptions) {
    this.compiler = new Compiler({
      globalEval: config.globalEval,
      global: config.global,
    });
  }

  run(path: string) {
    this.compiler.run(path);
  }

  runTests(paths: string[]) {
    for (const suitePath of paths) {
      loadSuite(suitePath);
      try {
        this.compiler.run(suitePath);
      } catch (err) {
        printSuiteError(suitePath, err);
        exit(1);
      }
    }
    run()
  }
}

/**
 * Install the fly development tools into the current runtime.
 */
export default function init(target: object, config: ConfigOptions) {
  const devTools = new FlyDevTools(config);

  Object.assign(target, {
    dev: devTools,
    ...globals
  });
}