import { Compiler } from "./compiler";
import { ConfigOptions, DevTools } from "../dev-tools";


class FlyDevTools implements DevTools {
  private compiler: Compiler;

  constructor(config: ConfigOptions) {
    this.compiler = new Compiler({
      globalEval: config.globalEval,
      global: config.global,
    });
  }

  run(path: string) {
    console.log(`RUN!`, { path });
    this.compiler.run(path);
  }
}

/**
 * Install the fly development tools into the current runtime.
 */
export default function init(config: ConfigOptions): DevTools {
  return new FlyDevTools(config);
}