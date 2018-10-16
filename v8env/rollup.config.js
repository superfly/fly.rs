import resolve from 'rollup-plugin-node-resolve';
import commonjs from 'rollup-plugin-commonjs';
import typescript from 'rollup-plugin-typescript2';
import sourceMaps from 'rollup-plugin-sourcemaps';
import builtins from 'rollup-plugin-node-builtins';
import json from "rollup-plugin-json";
import * as path from 'path';

export default [
  // {
  //   input: 'src/index.ts',
  //   output: {
  //     file: 'dist/v8env.js',
  //     format: 'iife',
  //     name: 'flyMain',
  //     sourcemap: true,
  //   },
  //   plugins: [
  //     typescript({ useTsconfigDeclarationDir: true }),
  //     resolve({
  //       jsnext: true,
  //     }),
  //     commonjs({
  //       include: './node_modules/**',
  //     }),
  //     sourceMaps(),
  //   ],
  //   watch: {
  //     include: 'src/**',
  //   }
  // },
  // {
  //   input: "src/test_main.ts",
  //   output: {
  //     file: 'dist/testing.js',
  //     format: 'iife',
  //     name: 'flyTest',
  //     sourcemap: true,
  //     globals: {
  //       mocha: 'mocha'
  //     }
  //   },
  //   plugins: [
  //     // builtins(),
  //     resolve({
  //       browser: true
  //       // jsnext: true,
  //       // module: true,
  //     }),
  //     commonjs({
  //       include: './node_modules/**',
  //     }),
  //     typescript({ useTsconfigDeclarationDir: true }),
  //     sourceMaps(),
  //   ]
  // },
  {
    input: "src/builder.ts",
    output: {
      file: 'dist/build.js',
      format: 'iife',
      name: 'flyBuild',
      sourcemap: true
    },
    globals: {
      rollup: 'rollup'
    },
    external: [
      "rollup"
    ],
    plugins: [
      resolve({
        jsnext: true,
      }),
      builtins(),
      commonjs({
        include: './node_modules/**',
      }),
      sourceMaps(),
      json(),
      typescript({ useTsconfigDeclarationDir: true }),
    ]
  }
];
