/**
 * Copyright 2018 Google Inc. All Rights Reserved.
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *     http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// If the loader is already loaded, just stop.
console.log("loaderjs.A")
if (!global.define) {
  console.log("loaderjs.B")
  const singleRequire = async name => {
    console.log("singleRequire.A", {name})
    if (!registry[name]) {
      console.log("singleRequire.B", { name })
      // #ifdef useEval
      const code = await fetch(name).then(resp => resp.text());
      console.log("singleRequire.C", { name })
      eval(code);
      console.log("singleRequire.D", { name })
      // #else
      await new Promise(async resolve => {
        console.log("singleRequire.E", { name })
        if ("document" in global) {
          console.log("singleRequire.F", { name })
          const script = document.createElement("script");
          // #ifdef publicPath
          script.src = // #put "'" + publicPath + "' + name.slice(1);"
            // #else
            script.src = name;
          // #endif
          // Ya never know
          script.defer = true;
          document.head.appendChild(script);
          script.onload = resolve;
        } else {
          console.log("singleRequire.G", { name })
          importScripts(name);
          resolve();
        }
      });
      // #endif

      console.log("singleRequire.H", { name })

      if (!registry[name]) {
        throw new Error(`Module ${name} didn’t register its module`);
      }
    }
    console.log("singleRequire.I", { name })
    return registry[name];
  };

  const require = async (names, resolve) => {
    console.log("require.A", {names})
    const modules = await Promise.all(names.map(singleRequire));
    resolve(modules.length === 1 ? modules[0] : modules);
  };

  const registry = {
    require: Promise.resolve(require)
  };

  global.define = (moduleName, depsNames, factory) => {
    console.log("define.A", { moduleName, depsNames, factory })
    if (registry[moduleName]) {
      console.log("define.B", { moduleName })
      // Module is already loading or loaded.
      return;
    }
    console.log("define.C", { moduleName })
    registry[moduleName] = new Promise(async resolve => {
      console.log("define.D", { moduleName })
      let exports = {};
      const deps = await Promise.all(
        depsNames.map(depName => {
          console.log("define.E", { moduleName, depName })
          if (depName === "exports") {
            console.log("define.F", { moduleName, depName })
            return exports;
          }
          console.log("define.G", { moduleName, depName })
          return singleRequire(depName);
        })
      );
      console.log("define.H", { moduleName })
      exports.default = factory(...deps);
      console.log("define.I", { moduleName })
      resolve(exports);
      console.log("define.J")
    });
  };
}
