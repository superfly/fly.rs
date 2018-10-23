export const loaderSrc = `
// If the loader is already loaded, just stop.
if (!global.define) {
  const singleRequire = async name => {
    if (!registry[name]) {
      await new Promise(async resolve => {
        importScripts(name);
        resolve();
      });

      if (!registry[name]) {
        throw new Error("Module " + name + "didn’t register its module");
      }
    }
    return registry[name];
  };

  const require = async (names, resolve) => {
    const modules = await Promise.all(names.map(singleRequire));
    resolve(modules.length === 1 ? modules[0] : modules);
  };

  const registry = {
    require: Promise.resolve(require)
  };

  global.define = (moduleName, depsNames, factory) => {
    if (registry[moduleName]) {
      // Module is already loading or loaded.
      return;
    }
    registry[moduleName] = new Promise(async resolve => {
      let exports = {};
      const deps = await Promise.all(
        depsNames.map(depName => {
          if (depName === "exports") {
            return exports;
          }
          return singleRequire(depName);
        })
      );
      exports.default = factory(...deps);
      resolve(exports);
    });
  };
}
`