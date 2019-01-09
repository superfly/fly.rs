/**
 * @module fly
 * @private
 */

import { libfly } from './libfly'
import { handleAsyncMsgFromRust } from "./bridge"
import * as sourceMaps from "./source_maps";

import "./globals";

function onGlobalError(
  message: string,
  source: string,
  lineno: number,
  colno: number,
  error: Error
) {
  console.log("Global Error", {message, source, lineno, colno, stack:error.stack});
}

export default function flyMain() {
  libfly.recv(handleAsyncMsgFromRust)
  libfly.setGlobalErrorHandler(onGlobalError);

  sourceMaps.install();
}