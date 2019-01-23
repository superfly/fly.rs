import * as ts from "typescript"

import { assert, assertNotNull, assertNotNullOrUndef, assertNotUndef } from "./util"
import { fetchModule } from "./resolver";
import { extname } from "./path";
import { ContainerName } from "./assets";

const EOL = "\n";

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

declare type ImportSpecifier = [ModuleSpecifier, ContainingFile];

enum MediaType {
  JavaScript = 0,
  TypeScript,
  Json,
  Unknown
}

const protocolPathPrefix: string = "protocol_";
const hostPathPrefix: string = "host_";

export function originUrlToFileName(originUrl: string): string {
  const parsedUrl = new URL(originUrl);
  return "/" + protocolPathPrefix + parsedUrl.protocol + "/" + hostPathPrefix + parsedUrl.host + parsedUrl.pathname + parsedUrl.search + parsedUrl.hash;
};

export function fileNameToOriginUrl(fileName: string): string {
  const fileNameParts = fileName.split("/");
  // These indexes had me a little confused at first. fileName should always start with a "/" so fileNameParts[0] = ""
  return fileNameParts[1].replace(protocolPathPrefix, "") + "//" + fileNameParts[2].replace(hostPathPrefix, "") + "/" + fileNameParts.slice(3).join("/");
}

class ModuleInfo implements ts.IScriptSnapshot {
  public inputCode: SourceCode = "";
  public outputCode?: OutputCode;
  public exports = {};
  public hasRun: boolean = false;
  public factory?: AmdFactory;
  public gatheringDeps = false;
  public deps?: ModuleId[];

  public constructor(
    public readonly originUrl: string,
    public version: number = 1,
    public readonly mediaType: MediaType = MediaType.Unknown
  ) {
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

  getText(start: number, end: number): string {
    return this.inputCode.substring(start, end)
  }

  getLength(): number {
    return this.inputCode.length;
  }

  getChangeRange(oldSnapshot: ts.IScriptSnapshot): ts.TextChangeRange | undefined {
    return
  }

  get fileName(): string {
    return originUrlToFileName(this.originUrl);
  }
}

class ModuleCache {
  // Maps module originUrl <==> ModuleInfo
  private readonly moduleIndex = new Map<string, ModuleInfo>();

  public get(originUrl: string): ModuleInfo {
    const moduleInfo = this.moduleIndex.get(originUrl);
    if (!moduleInfo) {
      throw new Error(`Module ${originUrl} not found`)
    }
    return moduleInfo
  }

  public getByFileName(fileName: string): ModuleInfo {
    return this.get(fileNameToOriginUrl(fileName));
  }

  public set(moduleInfo: ModuleInfo) {
    this.moduleIndex.set(moduleInfo.originUrl, moduleInfo);
  }
  
  public has(originUrl: ModuleFileName): boolean {
    return this.moduleIndex.has(originUrl);
  }
  
  public keys(): Array<string> {
    return Array.from(this.moduleIndex.keys());
  }
}

export interface CompilerOptions {
  globalEval: (string) => any;
  global: any;
}

export class Compiler {
  private readonly moduleCache = new ModuleCache();
  private readonly fileNameCache = new Map<ImportSpecifier, ModuleFileName>();
  private readonly runQueue: ModuleInfo[] = [];
  public scriptFileNames: string[] = [];
  private readonly globalEval: (string) => any;
  private readonly global: any;
  private readonly languageService = createLanguageService(this);

  public constructor(options: CompilerOptions) {
    this.global = options.global;
    this.globalEval = options.globalEval;
  }

  public run(specifierUrl: ModuleSpecifier, containingFile?: ContainingFile) {
    trace("run()", { specifierUrl, containingFile });
    // Load entry point module and put it's file name in the scriptFileNames field as a new array
    const moduleMetaData = this.resolveModule(specifierUrl, containingFile);
    this.scriptFileNames = [moduleMetaData.fileName];
    // If the module doesn't have any dependencies(hasn't been loaded before) instantiate it
    if (!moduleMetaData.deps) {
      this.instantiateModule(moduleMetaData);
    }
    // 
    this.drainRunQueue();
    return moduleMetaData;
  }

  public resolveModule(specifierUrl: string, refererOriginUrl?: string): ModuleInfo {
    trace("resolveModule()", { specifierUrl, refererOriginUrl })
    // attempt to load module from cache
    let fn = this.fileNameCache.get([specifierUrl, refererOriginUrl]);
    if (fn && this.moduleCache.has(fn)) {
      // return if found
      return this.moduleCache.get(fn);
    }
    let { originUrl, loadedSource } = fetchModule(specifierUrl, refererOriginUrl);

    // If module id is null or undef resolve failed.
    if (!originUrl) {
      throw new Error(`Failed to resolve '${specifierUrl}' from '${refererOriginUrl}'`);
    }

    // If module cache already contains module return it
    if (this.moduleCache.has(originUrl)) {
      return this.moduleCache.get(originUrl)
    }

    // Create new ModuleInfo object and fill it with info 
    const moduleInfo = new ModuleInfo(originUrl, 0, mediaType(originUrl));
    moduleInfo.inputCode = loadedSource.source;
    // Put module into cache for the next guy to pick it up
    this.moduleCache.set(moduleInfo);
    this.fileNameCache.set([specifierUrl, refererOriginUrl], originUrl);
    return moduleInfo;
  }

  getModuleInfo(originUrl: string): ModuleInfo {
    if (this.moduleCache.has(originUrl)) {
      return this.moduleCache.get(originUrl);
    }
    const moduleInfo = this.resolveModule(originUrl);
    this.moduleCache.set(moduleInfo);
    return moduleInfo;
  }

  getModuleInfoByFileName(fileName: string): ModuleInfo {
    return this.getModuleInfo(fileNameToOriginUrl(fileName));
  }

  /**
   * Retrieve the output of the TypeScript compiler for a given module and
   * cache the result. Re-compilation can be forced using '--recompile' flag.
   */
  compile(moduleInfo: ModuleInfo): OutputCode {
    trace("compile()", { moduleInfo });
    const recompile = false; // only relevant for persistent cache
    // If module already has ouputCode return that(Nothing to compile).
    if (!recompile && moduleInfo.outputCode) {
      return moduleInfo.outputCode;
    }
    const { originUrl, inputCode, fileName } = moduleInfo;
    const output = this.languageService.getEmitOutput(fileName);
    // Get the relevant diagnosetics - this is 3x faster than
    // `getPreEmitDiagnostics`.
    const diagnostics = [
      ...this.languageService.getCompilerOptionsDiagnostics(),
      ...this.languageService.getSyntacticDiagnostics(fileName),
      ...this.languageService.getSemanticDiagnostics(fileName)
    ];
    // If the language service reports log error and throw.
    if (diagnostics.length > 0) {
      const errMsg = ts.formatDiagnosticsWithColorAndContext(diagnostics, diagnosticHost);
      console.error("Compiler error", { errMsg });

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
    // Return compiled code.
    return moduleInfo.outputCode;
  }

  public transform(moduleId: string): string {
    trace("transform()", { moduleId });
    const moduleMetaData = this.resolveModule(moduleId);
    this.scriptFileNames = [moduleId];
    return this.compile(moduleMetaData)
  }

  /**
   * Drain the run queue, retrieving the arguments for the module
   * factory and calling the module's factory.
   */
  drainRunQueue(): void {
    trace(
      "drainRunQueue()",
      this.runQueue.map(moduleInfo => moduleInfo.originUrl)
    );
    // For each module in the runQueue 
    let moduleMetaData: ModuleInfo | undefined;
    while ((moduleMetaData = this.runQueue.shift())) {
      // Error if module has no factory or factory is null
      assertNotNullOrUndef(
        moduleMetaData.factory,
        "Cannot run module without factory."
      );
      assert(moduleMetaData.hasRun === false, "Module has already been run.");
      // asserts not tracked by TypeScripts, so using not null operator
      moduleMetaData.factory!(...this.getFactoryArguments(moduleMetaData));
      moduleMetaData.hasRun = true;
    }
  }

  /**
   * Get the dependencies for a given module, but don't run the module,
   * just add the module factory to the run queue.
   */
  instantiateModule(moduleInfo: ModuleInfo): void {
    trace("instantiateModule()", moduleInfo.originUrl);

    // if the module has already run, we can short circuit.
    // it is intentional though that if we have already resolved dependencies,
    // we won't short circuit, as something may have changed, or we might have
    // only collected the dependencies to be able to able to obtain the graph of
    // dependencies
    if (moduleInfo.hasRun) {
      return;
    }

    /**
     * I assume the global part has some use but it may be not longer be needed
     */
    this.global.define = this.makeDefine(moduleInfo);
    this.globalEval(this.compile(moduleInfo));
    this.global.define = undefined;
  }

  /**
   * Retrieve the arguments to pass a module's factory function.
   */
  getFactoryArguments(moduleMetaData: ModuleInfo): any[] {
    // return []
    if (!moduleMetaData.deps) {
      throw new Error("Cannot get arguments until dependencies resolved.");
    }
    // For each dependency 
    return moduleMetaData.deps.map(dep => {
      if (dep === "require") {
        return this.makeLocalRequire(moduleMetaData);
      }
      if (dep === "exports") {
        return moduleMetaData.exports;
      }
      // if (dep in DenoCompiler._builtins) {
      //   return DenoCompiler._builtins[dep];
      // }
      const dependencyMetaData = this.getModuleInfoByFileName(dep);
      assert(dependencyMetaData != null, `Missing dependency "${dep}".`);
      // TypeScript does not track assert, therefore using not null operator
      return dependencyMetaData!.exports;
    });
  }

  /**
   * Create a localized AMD `define` function and return it.
   */
  makeDefine(moduleInfo: ModuleInfo): AmdDefine {
    return (deps: ModuleSpecifier[], factory: AmdFactory): void => {
      console.trace("compiler.localDefine", moduleInfo.fileName);
      moduleInfo.factory = factory;
      // when there are circular dependencies, we need to skip recursing the
      // dependencies
      moduleInfo.gatheringDeps = true;
      // we will recursively resolve the dependencies for any modules and store them as file names
      moduleInfo.deps = deps.map(dep => {
        if (
          dep === "require" ||
          dep === "exports" //||
          // dep in DenoCompiler._builtins
        ) {
          return dep;
        }
        // Resolve the dependency and instantiate it if not currently gathering deps to avoid loading deps more than once
        const dependencyMetaData = this.resolveModule(dep, moduleInfo.originUrl);
        if (!dependencyMetaData.gatheringDeps) {
          this.instantiateModule(dependencyMetaData);
        }
        // Return the resolved dep's fileName
        return dependencyMetaData.fileName;
      });
      // Remove gatheringDeps lock and if the runQueue doesn't already contain this module add it.
      moduleInfo.gatheringDeps = false;
      if (!this.runQueue.includes(moduleInfo)) {
        this.runQueue.push(moduleInfo);
      }
    };
  }

  /**
   * Returns a require that specifically handles the resolution of a transpiled
   * emit of a dynamic ES `import()` from TypeScript.
   */
  makeLocalRequire(moduleInfo: ModuleInfo): AMDRequire {
    return (
      deps: ModuleSpecifier[],
      callback: AmdCallback,
      errback: AmdErrback
    ): void => {
      console.trace("compiler.makeLocalRequire()", { moduleInfo, deps });
      assert(
        deps.length === 1,
        "Local require requires exactly one dependency."
      );
      const [moduleSpecifier] = deps;
      try {
        const requiredMetaData = this.run(moduleSpecifier, moduleInfo.originUrl);
        callback(requiredMetaData.exports);
      } catch (e) {
        errback(e);
      }
    };
  }
}

const settings: ts.CompilerOptions = {
  allowJs: true,
  module: ts.ModuleKind.AMD,
  // module: ts.ModuleKind.ESNext,
  outDir: "$fly$",
  baseUrl: "",
  inlineSourceMap: true,
  inlineSources: true,
  stripComments: true,
  target: ts.ScriptTarget.ESNext,
};

function createLanguageService(compiler: Compiler): ts.LanguageService {
  return ts.createLanguageService({
    getCompilationSettings(): ts.CompilerOptions {
      return settings;
    },
    getScriptFileNames(): string[] {
      return compiler.scriptFileNames;
    },
    getScriptVersion(fileName: string): string {
      trace("getScriptVersion()", { fileName })
      const moduleInfo = compiler.getModuleInfoByFileName(fileName);
      if (!moduleInfo) {
        return ""
      }
      return moduleInfo.version.toString();
    },
    getScriptSnapshot(fileName: string): ts.IScriptSnapshot | undefined {
      trace("getScriptSnapshot()", { fileName })
      return compiler.getModuleInfoByFileName(fileName);
    },
    getCurrentDirectory(): string {
      return ""
    },
    getDefaultLibFileName(options: ts.CompilerOptions): string {
      trace("getDefaultLibFileName()");
      const moduleSpecifier = "lib.fly.runtime.d.ts";
      const moduleInfo = compiler.resolveModule(ContainerName + moduleSpecifier);
      return moduleInfo.fileName;
    },
    getNewLine: (): string => {
      return EOL;
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
      trace("resolveModuleNames()", { moduleNames, containingFile, reusedNames });

      return moduleNames.map(moduleName => {
        const moduleInfo = compiler.resolveModule(moduleName, fileNameToOriginUrl(containingFile))
        // an empty string will cause typescript to bomb, maybe fail here instead?
        const resolvedFileName = moduleInfo && moduleInfo.fileName || ""
        const isExternalLibraryImport = false; // need cwd/cjs logic for this maybe?
        return { resolvedFileName, isExternalLibraryImport }
      })
    },
    getScriptKind(fileName: string): ts.ScriptKind {
      trace("getScriptKind()", { fileName });
      const moduleMetaData = compiler.getModuleInfoByFileName(fileName);
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
      console.trace("Typescript ls doing file exists check.");
      const info = compiler.getModuleInfoByFileName(path);
      const exists = info != null;
      trace("fileExists()", { path, exists });
      return exists;
    },
  })
}

const diagnosticHost: ts.FormatDiagnosticsHost = {
  getNewLine: () => EOL,
  getCurrentDirectory: () => "",
  getCanonicalFileName: (path) => path
}

// TODO: move this to resolver?
function mediaType(originUrl): MediaType {
  switch (extname(originUrl)) {
    case ".ts": return MediaType.TypeScript;
    case ".js": return MediaType.JavaScript;
    case ".json": return MediaType.Json;
  }
  return MediaType.Unknown;
}

function trace(...args: any[]) {
  console.trace("[compiler]", ...args);
}