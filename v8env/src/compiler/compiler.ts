import * as ts from "typescript"

import { assert } from "./util"
import { window } from "../globals"
import { globalEval } from "../global-eval"
import { fetchModule } from "./resolver";
import { extname } from "./path";
import { ContainerName } from "./assets";

type AmdCallback = (...args: unknown[]) => void;
type AmdErrback = (err: unknown) => void;
export type AmdFactory = (...args: unknown[]) => object | void;
export type AmdDefine = (deps: ModuleSpecifier[], factory: AmdFactory) => void;
type AMDRequire = (deps: ModuleSpecifier[], callback: AmdCallback, errback: AmdErrback) => void;

/**
 * The location that a module is being loaded from. This could be a directory,
 * like `.`, or it could be a module specifier like
 * `http://gist.github.com/somefile.ts`
 */
type ContainingFile = string;
/**
 * The internal local filename of a compiled module. It will often be something
 * like `/home/ry/.deno/gen/f7b4605dfbc4d3bb356e98fda6ceb1481e4a8df5.js`
 */
type ModuleFileName = string;
/**
 * The original resolved resource name.
 * Path to cached module file or URL from which dependency was retrieved
 */
type ModuleId = string;
/**
 * The external name of a module - could be a URL or could be a relative path.
 * Examples `http://gist.github.com/somefile.ts` or `./somefile.ts`
 */
type ModuleSpecifier = string;
/**
 * The compiled source code which is cached in `.deno/gen/`
 */
type OutputCode = string;
/**
 * The original source code
 */
type SourceCode = string;

enum MediaType {
  JavaScript = 0,
  TypeScript,
  Json,
  Unknown
}

class ModuleInfo implements ts.IScriptSnapshot {
  public moduleId: ModuleId;
  public version: number = 1;
  public inputCode: SourceCode = "";
  public outputCode?: OutputCode;
  public exports = {};
  public hasRun: boolean = false;
  public factory?: AmdFactory;
  public gatheringDeps = false;
  public deps?: ModuleId[];
  public readonly mediaType: MediaType;

  public constructor(moduleId: ModuleId, version?: number, type?: MediaType)
  {
    this.moduleId = moduleId;
    this.version = version || 1;
    this.mediaType = type;
  }

  reload() {
    this.hasRun = false;
    this.exports = {}
    this.factory = undefined;
    this.gatheringDeps = false;
    this.deps = undefined
    this.version += 1
    this.outputCode = undefined
  }

  get fileName() {
    return this.moduleId
  }

  getText(start: number, end: number): string {
    return this.inputCode.substring(start, end)
  }

  getLength(): number {
    return this.inputCode.length;
  }

  getChangeRange(oldSnapshot: ts.IScriptSnapshot): ts.TextChangeRange | undefined {
    return
  }
}

class ModuleCache {
  private readonly moduleIndex = new Map<string, ModuleInfo>();

  public get(moduleId: string): ModuleInfo {
    const moduleInfo = this.moduleIndex.get(moduleId);
    if (!moduleInfo) {
      throw new Error(`Module ${moduleId} not found`)
    }
    return moduleInfo
  }

  public set(moduleInfo: ModuleInfo) {
    this.moduleIndex.set(moduleInfo.moduleId, moduleInfo);
  }
  
  public has(moduleId: string): boolean {
    return this.moduleIndex.has(moduleId);
  }

  public moduleIds(): string[] {
    return Array.from(this.moduleIndex.keys());
  }
}

const settings: ts.CompilerOptions = {
  allowJs: true,
  module: ts.ModuleKind.AMD,
  // module: ts.ModuleKind.ESNext,
  outDir: "$fly$",
  // TODO https://github.com/denoland/deno/issues/23
  inlineSourceMap: true,
  inlineSources: true,
  stripComments: true,
  target: ts.ScriptTarget.ESNext
}

const cache = new ModuleCache();

function getModuleInfo(moduleId: string): ModuleInfo {
  console.log("compiler.getModuleInfo()", { moduleId })

  if (cache.has(moduleId)) {
    return cache.get(moduleId);
  }
  const moduleInfo = resolveModule(moduleId, "")
  cache.set(moduleInfo)
  return moduleInfo
}

export function resolveModule(moduleSpecifier: string, containingFile: string): ModuleInfo {
  console.log("compiler.resolveModule()", { moduleSpecifier, containingFile })
  const { moduleId, sourceCode } = fetchModule(moduleSpecifier, containingFile);

  console.log("compiler.resolveModule()", {moduleId, sourceCode})
  if (!moduleId) {
    throw new Error(`Failed to resolve '${moduleSpecifier}' from '${containingFile}'`)
  }

  if (cache.has(moduleId)) {
    return cache.get(moduleId)
  }

  const moduleInfo = new ModuleInfo(moduleId, 0, mediaType(moduleId))
  moduleInfo.inputCode = sourceCode
  cache.set(moduleInfo)
  return moduleInfo
}

// TODO: move this to resolver?
function mediaType(moduleId): MediaType {
  switch (extname(moduleId)) {
    case ".ts": return MediaType.TypeScript;
    case ".js": return MediaType.JavaScript;
    case ".json": return MediaType.Json;
  }
  return MediaType.Unknown;
}

let scriptFileNames: string[] = []

const service = ts.createLanguageService({
  // Required
  getCompilationSettings(): ts.CompilerOptions {
    return settings;
  },
  getScriptFileNames(): string[] {
    return scriptFileNames;
  },
  getScriptVersion(fileName: string): string {
    console.log("compiler.getScriptVersion()", { fileName })
    const moduleInfo = getModuleInfo(fileName);
    if (!moduleInfo) {
      return ""
    }
    return moduleInfo.version.toString();
  },
  getScriptSnapshot(fileName: string): ts.IScriptSnapshot | undefined {
    console.log("compiler.getScriptSnapshot()", { fileName })
    return getModuleInfo(fileName)
  },
  getCurrentDirectory(): string {
    return ""
  },
  getDefaultLibFileName(options: ts.CompilerOptions): string {
    console.log("getDefaultLibFileName()");
    const moduleSpecifier = "lib.fly.runtime.d.ts";
    const moduleInfo = resolveModule(moduleSpecifier, ContainerName);
    return moduleInfo.fileName;
  },

  // optional

  getNewLine: (): string => {
    return "\n";
  },
  log(s: string): void {
    console.log("[compilerHost]", s)
  },
  trace(s: string): void {
    console.trace("[compilerHost]", s)
  },
  error(s: string): void {
    console.error("[compilerHost]", s)
  },
  resolveModuleNames(moduleNames: string[], containingFile: string, reusedNames?: string[]): ts.ResolvedModule[] {
    console.log("[compiler] resolveModuleNames", { moduleNames, containingFile, reusedNames })
    
    return moduleNames.map(moduleName => {
      console.log("RESOLVING", {moduleName})
      const moduleInfo = resolveModule(moduleName, containingFile)
      // an empty string will cause typescript to bomb, maybe fail here instead?
      const resolvedFileName = moduleInfo && moduleInfo.moduleId || ""
      const isExternal = false; // need cwd/cjs logic for this maybe?
      console.log("ENDED UP WOTH", { moduleName, isExternal, resolvedFileName });
      return { resolvedFileName, isExternal }
    })
  },
  getScriptKind(fileName: string): ts.ScriptKind {
    console.log("getScriptKind()", fileName);
    const moduleMetaData = getModuleInfo(fileName);
    if (moduleMetaData) {
      switch (moduleMetaData.mediaType) {
        case MediaType.TypeScript:
          return ts.ScriptKind.TS;
        case MediaType.JavaScript:
          return ts.ScriptKind.JS;
        case MediaType.Json:
          return ts.ScriptKind.JSON;
        default:
          return settings.allowJs ? ts.ScriptKind.JS : ts.ScriptKind.TS;
      }
    } else {
      return settings.allowJs ? ts.ScriptKind.JS : ts.ScriptKind.TS;
    }
  },

  useCaseSensitiveFileNames(): boolean {
    return true;
  },
  fileExists(path: string): boolean {
    const info = getModuleInfo(path);
    const exists = info != null;
    console.log("fileExists()", path, exists);
    return exists;
  },
})

const diagnosticHost: ts.FormatDiagnosticsHost = {
  getNewLine: () => "\n",
  getCurrentDirectory: () => "",
  getCanonicalFileName: (path) => path
}

/**
 * Retrieve the output of the TypeScript compiler for a given module and
 * cache the result. Re-compilation can be forced using '--recompile' flag.
 */
function compile(moduleInfo: ModuleInfo): OutputCode {
  const recompile = false; // only relevant for persistent cache
  if (!recompile && moduleInfo.outputCode) {
    return moduleInfo.outputCode;
  }
  const { fileName, inputCode, moduleId } = moduleInfo;
  console.warn("Compiling", {moduleId, fileName});
  const output = service.getEmitOutput(fileName);
  console.warn("COMPILING DONE EMIT", { output})
  // Get the relevant diagnostics - this is 3x faster than
  // `getPreEmitDiagnostics`.
  const diagnostics = [
    ...service.getCompilerOptionsDiagnostics(),
    ...service.getSyntacticDiagnostics(fileName),
    ...service.getSemanticDiagnostics(fileName)
  ];
  if (diagnostics.length > 0) {
    const errMsg = ts.formatDiagnosticsWithColorAndContext(diagnostics, diagnosticHost);
    console.error("Compiler error", { errMsg } );

    throw new Error("typescript error, quit")
    // this._os.exit(1);
  }

  assert(!output.emitSkipped, "The emit was skipped for an unknown reason.");

  // Currently we are inlining source maps, there should be only 1 output file
  // See: https://github.com/denoland/deno/issues/23
  assert(
    output.outputFiles.length === 1,
    "Only single file should be output."
  );

  const [outputFile] = output.outputFiles;
  const outputCode = (moduleInfo.outputCode = `${
    outputFile.text
    }\n//# sourceURL=${fileName}`);
  moduleInfo.version = 1;
  // write to persistent cache
  // this._os.codeCache(fileName, sourceCode, outputCode);
  return moduleInfo.outputCode;
}

export function run(moduleSpecifier: ModuleSpecifier, containingFile: ContainingFile): ModuleInfo {
  console.log("compiler.run", { moduleSpecifier, containingFile });
  const moduleMetaData = resolveModule(moduleSpecifier, containingFile);
  scriptFileNames = [moduleMetaData.fileName];
  if (!moduleMetaData.deps) {
    instantiateModule(moduleMetaData);
  }
  drainRunQueue();
  return moduleMetaData;
}

export function reload(moduleId: ModuleId): ModuleInfo {
  const moduleInfo = getModuleInfo(moduleId)
  moduleInfo.reload()
  return moduleInfo
}

export function transform(moduleId: string): string {
  console.log("compiler.transform", { moduleId });
  const moduleMetaData = resolveModule(moduleId, "");
  scriptFileNames = [moduleId];
  return compile(moduleMetaData)
}

export function dump() {
  cache.moduleIds().map(id => ({ id: id, moduleInfo: cache.get(id) }))
}

export const moduleCache = cache;

const runQueue: ModuleInfo[] = [];

/**
 * Drain the run queue, retrieving the arguments for the module
 * factory and calling the module's factory.
 */
function drainRunQueue(): void {
  console.log(
    "compiler.drainRunQueue",
    runQueue.map(moduleInfo => moduleInfo.moduleId)
  );
  let moduleMetaData: ModuleInfo | undefined;
  while((moduleMetaData = runQueue.shift())) {
    assert(
      moduleMetaData.factory != null,
      "Cannot run module without factory."
    );
    assert(moduleMetaData.hasRun === false, "Module has already been run.");
    // asserts not tracked by TypeScripts, so using not null operator
    moduleMetaData.factory!(...getFactoryArguments(moduleMetaData));
    moduleMetaData.hasRun = true;
  }
}

/**
 * Get the dependencies for a given module, but don't run the module,
 * just add the module factory to the run queue.
 */
function instantiateModule(moduleInfo: ModuleInfo): void {
  console.log("compiler.instantiateModule", moduleInfo.moduleId);

  // if the module has already run, we can short circuit.
  // it is intentional though that if we have already resolved dependencies,
  // we won't short circuit, as something may have changed, or we might have
  // only collected the dependencies to be able to able to obtain the graph of
  // dependencies
  if (moduleInfo.hasRun) {
    return;
  }

  window.define = makeDefine(moduleInfo);
  console.log("START COMPILE", {moduleInfo})
  globalEval(compile(moduleInfo));
  console.log("END COMPILE", { moduleInfo })
  window.define = undefined;
}

/**
 * Retrieve the arguments to pass a module's factory function.
 */
function getFactoryArguments(moduleMetaData: ModuleInfo): any[] {
    // return []
  if (!moduleMetaData.deps) {
    throw new Error("Cannot get arguments until dependencies resolved.");
  }
  return moduleMetaData.deps.map(dep => {
    if (dep === "require") {
      return makeLocalRequire(moduleMetaData);
    }
    if (dep === "exports") {
      return moduleMetaData.exports;
    }
    // if (dep in DenoCompiler._builtins) {
    //   return DenoCompiler._builtins[dep];
    // }
    const dependencyMetaData = getModuleInfo(dep);
    assert(dependencyMetaData != null, `Missing dependency "${dep}".`);
    // TypeScript does not track assert, therefore using not null operator
    return dependencyMetaData!.exports;
  });
}

/**
 * Create a localized AMD `define` function and return it.
 */
function makeDefine(moduleInfo: ModuleInfo): AmdDefine {
  return (deps: ModuleSpecifier[], factory: AmdFactory): void => {
    console.log("compiler.localDefine", moduleInfo.fileName);
    moduleInfo.factory = factory;
    // when there are circular dependencies, we need to skip recursing the
    // dependencies
    moduleInfo.gatheringDeps = true;
    // we will recursively resolve the dependencies for any modules
    moduleInfo.deps = deps.map(dep => {
      if (
        dep === "require" ||
        dep === "exports" //||
        // dep in DenoCompiler._builtins
      ) {
        return dep;
      }
      const dependencyMetaData = resolveModule(dep, moduleInfo.fileName);
      if (!dependencyMetaData.gatheringDeps) {
        instantiateModule(dependencyMetaData);
      }
      return dependencyMetaData.fileName;
    });
    moduleInfo.gatheringDeps = false;
    if (!runQueue.includes(moduleInfo)) {
      runQueue.push(moduleInfo);
    }
  };
}

/**
 * Returns a require that specifically handles the resolution of a transpiled
 * emit of a dynamic ES `import()` from TypeScript.
 */
function makeLocalRequire(moduleInfo: ModuleInfo): AMDRequire {
  return (
    deps: ModuleSpecifier[],
    callback: AmdCallback,
    errback: AmdErrback
  ): void => {
    console.log("compiler.makeLocalRequire()", { moduleInfo, deps });
    assert(
      deps.length === 1,
      "Local require requires exactly one dependency."
    );
    const [moduleSpecifier] = deps;
    try {
      const requiredMetaData = run(moduleSpecifier, moduleInfo.fileName);
      callback(requiredMetaData.exports);
    } catch (e) {
      errback(e);
    }
  };
}

let scriptVersion = 1;

export function cacheScript(source: string) {
  const moduleInfo = new ModuleInfo("playground.ts", scriptVersion++)
  moduleInfo.inputCode = source;
  cache.set(moduleInfo)
}
