import resolvePlugin from 'rollup-plugin-node-resolve';
import commonjsPlugin from 'rollup-plugin-commonjs';
import typescriptPlugin from 'rollup-plugin-typescript2';
import sourceMaps from 'rollup-plugin-sourcemaps';
import builtins from 'rollup-plugin-node-builtins';
import * as path from 'path';
import virtual from 'rollup-plugin-virtual';
import { createFilter } from "rollup-pluginutils";
import globalsPlugin from "rollup-plugin-node-globals";
import { execSync } from 'child_process';
import typescript from "typescript";
import MagicString from "magic-string";

const typescriptPath = path.resolve(
  __dirname,
  "node_modules/typescript/lib/typescript.js"
);

const mock = `export default undefined;`

/** this is a rollup plugin which will look for imports ending with `!string` and resolve
 * them with a module that will inline the contents of the file as a string.  Needed to
 * support `js/assets.ts`.
 * @param {any} param0
 */
function strings(
  { include, exclude } = { include: undefined, exclude: undefined }
) {
  if (!include) {
    throw new Error("include option must be passed");
  }

  const filter = createFilter(include, exclude);

  return {
    name: "strings",

    /**
     * @param {string} importee
     */
    resolveId(importee) {
      if (importee.endsWith("!string")) {
        // strip the `!string` from `importee`
        importee = importee.slice(0, importee.lastIndexOf("!string"));
        if (!importee.startsWith("gen/")) {
          // this is a static asset which is located relative to the root of
          // the source project
          return path.resolve(path.join(__dirname, importee));
        }
        // this is an asset which has been generated, therefore it will be
        // located within the build path
        return path.resolve(path.join(process.cwd(), importee));
      }
    },

    /**
     * @param {any} code
     * @param {string} id
     */
    transform(code, id) {
      if (filter(id)) {
        return {
          code: `export default ${JSON.stringify(code)};`,
          map: { mappings: "" }
        };
      }
    }
  };
}

function runtimeInfo(path) {
  const filter = createFilter([path]);

  return {
    name: "runtimeInfo",
    transform: (code, id) => {
      if (filter(id)) {
        const build = execSync('./scripts/build-number.sh', {
          cwd: '..'
        }).toString();

        const magicString = new MagicString(`export const runtime = { build: "${build}" };`);

        return {
          code: magicString.toString(),
          map: magicString.generateMap()
        };
      }
    }
  }
}

export default [
  {
    input: 'src/index.ts',
    output: {
      file: 'dist/v8env.js',
      format: 'iife',
      name: 'flyMain',
      sourcemap: true,
    },
    plugins: [
      runtimeInfo(path.resolve(__dirname, "src/runtime.ts")),
      typescriptPlugin({ useTsconfigDeclarationDir: true }),
      resolvePlugin({
        jsnext: true,
      }),
      commonjsPlugin({
        include: './node_modules/**',
      }),
      sourceMaps(),
    ],
    watch: {
      include: 'src/**',
    }
  },
  {
    input: "src/test_main.ts",
    output: {
      file: 'dist/testing.js',
      format: 'iife',
      name: 'flyTest',
      sourcemap: true,
      globals: {
        mocha: 'mocha'
      }
    },
    plugins: [
      // builtins(),
      resolvePlugin({
        browser: true
        // jsnext: true,
        // module: true,
      }),
      commonjsPlugin({
        include: './node_modules/**',
      }),
      typescriptPlugin({ useTsconfigDeclarationDir: true }),
      sourceMaps(),
    ]
  },
  {
    input: "src/dev-tools/index.ts",
    output: {
      file: 'dist/dev-tools.js',
      format: 'iife',
      name: 'devTools',
      sourcemap: true,
      sourcemapExcludeSources: true,
      globals: {
        rollup: "rollup"
      }
    },
    external: [
      'rollup'
    ],
    plugins: [
      virtual({
        fs: mock,
        path: mock,
        os: mock,
        crypto: mock,
        buffer: mock,
        module: mock
      }),

      // alias({
      //   path: path.resolve(__dirname, "node_modules/path-browserify/index.js"),
      //   // rollup: rollupPath
      // }),

      // Provides inlining of file contents for `js/assets.ts`
      strings({
        include: [
          "*.d.ts",
          `${__dirname}/**/*.d.ts`,
          `${process.cwd()}/**/*.d.ts`
        ]
      }),

      resolvePlugin({
        jsnext: true,
        main: true
        // browser: true
      }),

      // Allows rollup to import CommonJS modules
      commonjsPlugin({
        // include: './node_modules/**',
        namedExports: {
          // Static analysis of `typescript.js` does detect the exports properly, therefore
          // rollup requires them to be explicitly defined to make them available in the
          // bundle
          [typescriptPath]: [
            "createLanguageService",
            "formatDiagnosticsWithColorAndContext",
            "ModuleKind",
            "ScriptKind",
            "ScriptSnapshot",
            "ScriptTarget",
            "version"
          ]
        }
      }),

      typescriptPlugin({
        useTsconfigDeclarationDir: true,
        typescript
        // tsconfigOverride: {
        //   module: "esnext"
        // }
      }),

      globalsPlugin({
        dirname: false,
        filename: false,
      }),

      // sourceMaps(),
    ]
  }
];
