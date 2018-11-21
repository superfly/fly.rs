import { globalEval, GlobalEval } from "./global-eval"; 
import { window } from "./globals";

export interface ConfigOptions {
  globalEval: GlobalEval;
  global: any;
}

export interface DevTools {
  run(path: string);
}

export type initFn = (config: ConfigOptions) => DevTools;

declare var devTools: initFn | undefined;

/**
 * Install the fly development tools into the current runtime.
 * The dev tools (typescript compiler, moduler loader, bundler, etc) are in
 * the flyDev.js bundle which exists outside the libfly scope, thus all 
 * globals from libfly need to be diretly passed into this method to ensure 
 * code is loaded in the right context. The flyDev.js bundle needs to be 
 * loaded from Rust before calling this function.
 */
export function installDevTools() {
  if (typeof devTools === "undefined") {
    throw Error("Dev tools are not available in this environment");
  }
  window.dev = devTools({
    global: window,
    globalEval
  });
}
