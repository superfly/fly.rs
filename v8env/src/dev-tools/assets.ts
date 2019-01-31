// tslint:disable-next-line:no-reference
/// <reference path="./assets.d.ts" />

import lib from "node_modules/typescript/lib/lib.d.ts!string";
import libDom from "node_modules/typescript/lib/lib.dom.d.ts!string";
import libDomIterable from "node_modules/typescript/lib/lib.dom.iterable.d.ts!string";
import libEs2015Collection from "node_modules/typescript/lib/lib.es2015.collection.d.ts!string";
import libEs2015Core from "node_modules/typescript/lib/lib.es2015.core.d.ts!string";
import libEs2015 from "node_modules/typescript/lib/lib.es2015.d.ts!string";
import libEs2015Generator from "node_modules/typescript/lib/lib.es2015.generator.d.ts!string";
import libEs2015Iterable from "node_modules/typescript/lib/lib.es2015.iterable.d.ts!string";
import libEs2015Promise from "node_modules/typescript/lib/lib.es2015.promise.d.ts!string";
import libEs2015Proxy from "node_modules/typescript/lib/lib.es2015.proxy.d.ts!string";
import libEs2015Reflect from "node_modules/typescript/lib/lib.es2015.reflect.d.ts!string";
import libEs2015Symbol from "node_modules/typescript/lib/lib.es2015.symbol.d.ts!string";
import libEs2015SymbolWellknown from "node_modules/typescript/lib/lib.es2015.symbol.wellknown.d.ts!string";
import libEs2016ArrayInclude from "node_modules/typescript/lib/lib.es2016.array.include.d.ts!string";
import libEs2016 from "node_modules/typescript/lib/lib.es2016.d.ts!string";
import libEs2016Full from "node_modules/typescript/lib/lib.es2016.full.d.ts!string";
import libEs2017 from "node_modules/typescript/lib/lib.es2017.d.ts!string";
import libEs2017Full from "node_modules/typescript/lib/lib.es2017.full.d.ts!string";
import libEs2017Intl from "node_modules/typescript/lib/lib.es2017.intl.d.ts!string";
import libEs2017Object from "node_modules/typescript/lib/lib.es2017.object.d.ts!string";
import libEs2017Sharedmemory from "node_modules/typescript/lib/lib.es2017.sharedmemory.d.ts!string";
import libEs2017String from "node_modules/typescript/lib/lib.es2017.string.d.ts!string";
import libEs2017Typedarrays from "node_modules/typescript/lib/lib.es2017.typedarrays.d.ts!string";
import libEs2018 from "node_modules/typescript/lib/lib.es2018.d.ts!string";
import libEs2018Full from "node_modules/typescript/lib/lib.es2018.full.d.ts!string";
import libEs2018Intl from "node_modules/typescript/lib/lib.es2018.intl.d.ts!string";
import libEs2018Promise from "node_modules/typescript/lib/lib.es2018.promise.d.ts!string";
import libEs2018Regexp from "node_modules/typescript/lib/lib.es2018.regexp.d.ts!string";
import libEs5 from "node_modules/typescript/lib/lib.es5.d.ts!string";
import libEs6 from "node_modules/typescript/lib/lib.es6.d.ts!string";
import libEsnextArray from "node_modules/typescript/lib/lib.esnext.array.d.ts!string";
import libEsnextAsynciterable from "node_modules/typescript/lib/lib.esnext.asynciterable.d.ts!string";
import libEsnext from "node_modules/typescript/lib/lib.esnext.d.ts!string";
import libEsnextFull from "node_modules/typescript/lib/lib.esnext.full.d.ts!string";
import libEsnextIntl from "node_modules/typescript/lib/lib.esnext.intl.d.ts!string";
import libEsnextSymbol from "node_modules/typescript/lib/lib.esnext.symbol.d.ts!string";
import libScripthost from "node_modules/typescript/lib/lib.scripthost.d.ts!string";
import libWebworker from "node_modules/typescript/lib/lib.webworker.d.ts!string";
import libWebworkerImportscripts from "node_modules/typescript/lib/lib.webworker.importscripts.d.ts!string";

import libFlyRuntime from "lib.fly.runtime.d.ts!string";

export const AssetsProtocol = "assets";
export const ContainerName = AssetsProtocol + "://local/"; // < I had to include a host for the url parser to do what I wanted.

// // @internal
export const assetSourceCode: { [key: string]: string } = {
  "lib.d.ts": lib,
  "lib.dom.d.ts": libDom,
  "lib.dom.iterable.d.ts": libDomIterable,
  "lib.es2015.collection.d.ts": libEs2015Collection,
  "lib.es2015.core.d.ts": libEs2015Core,
  "lib.es2015.d.ts": libEs2015,
  "lib.es2015.generator.d.ts": libEs2015Generator,
  "lib.es2015.iterable.d.ts": libEs2015Iterable,
  "lib.es2015.promise.d.ts": libEs2015Promise,
  "lib.es2015.proxy.d.ts": libEs2015Proxy,
  "lib.es2015.reflect.d.ts": libEs2015Reflect,
  "lib.es2015.symbol.d.ts": libEs2015Symbol,
  "lib.es2015.symbol.wellknown.d.ts": libEs2015SymbolWellknown,
  "lib.es2016.array.include.d.ts": libEs2016ArrayInclude,
  "lib.es2016.d.ts": libEs2016,
  "lib.es2016.full.d.ts": libEs2016Full,
  "lib.es2017.d.ts": libEs2017,
  "lib.es2017.full.d.ts": libEs2017Full,
  "lib.es2017.intl.d.ts": libEs2017Intl,
  "lib.es2017.object.d.ts": libEs2017Object,
  "lib.es2017.sharedmemory.d.ts": libEs2017Sharedmemory,
  "lib.es2017.string.d.ts": libEs2017String,
  "lib.es2017.typedarrays.d.ts": libEs2017Typedarrays,
  "lib.es2018.d.ts": libEs2018,
  "lib.es2018.full.d.ts": libEs2018Full,
  "lib.es2018.intl.d.ts": libEs2018Intl,
  "lib.es2018.promise.d.ts": libEs2018Promise,
  "lib.es2018.regexp.d.ts": libEs2018Regexp,
  "lib.es5.d.ts": libEs5,
  "lib.es6.d.ts": libEs6,
  "lib.esnext.array.d.ts": libEsnextArray,
  "lib.esnext.asynciterable.d.ts": libEsnextAsynciterable,
  "lib.esnext.d.ts": libEsnext,
  "lib.esnext.full.d.ts": libEsnextFull,
  "lib.esnext.intl.d.ts": libEsnextIntl,
  "lib.esnext.symbol.d.ts": libEsnextSymbol,
  "lib.scripthost.d.ts": libScripthost,
  "lib.webworker.d.ts": libWebworker,
  "lib.webworker.importscripts.d.ts": libWebworkerImportscripts,

  "lib.fly.runtime.d.ts": libFlyRuntime,
};
