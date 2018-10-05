import resolve from 'rollup-plugin-node-resolve';
import commonjs from 'rollup-plugin-commonjs';
import typescript from 'rollup-plugin-typescript2';
import sourceMaps from 'rollup-plugin-sourcemaps';
import * as path from 'path';

export default {
  input: 'src/index.ts',
  plugins: [
    typescript({ useTsconfigDeclarationDir: true }),
    resolve({
      jsnext: true,
      customResolveOptions: {
        moduleDirectory: '../../node_modules'
      }
    }),
    commonjs({
      include: '../../node_modules/**',
    }),
    sourceMaps(),
  ],
  watch: {
    include: 'src/**',
  },
  output: {
    file: 'dist/v8env.js',
    format: 'iife',
    name: 'flyMain',
    sourcemap: true,
  }
};
