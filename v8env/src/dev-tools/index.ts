import { Compiler } from "./compiler";
import { run, globals } from "./testing";
import { DevTools, ConfigOptions } from "./api";

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

  runTests() {
    run();
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