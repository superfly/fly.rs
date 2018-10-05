// Copyright 2018 the Deno authors. All rights reserved. MIT license.

import { Console } from "./console";
import * as timers from "./timers";
// import * as textEncoding from "./text_encoding";
// import * as fetch_ from "./fetch";
import { libfly } from "./libfly";
import { globalEval } from "./global-eval";
import * as bridge from "./bridge";
import * as textEncoding from "./text-encoding";
import { FlyResponse } from "./response";
import * as fetch_ from './fetch';
import * as resolv_ from './resolv';
import * as dns from './dns';
import * as crypto_ from "./crypto";
import cache_ from "./cache";

import * as url from './url';
import { FlyRequest } from "./request";
import data from './fly/data';

declare global {
  interface Window {
    console: Console;
    define: Readonly<unknown>;
  }

  //   const clearTimeout: typeof timers.clearTimer;
  //   const clearInterval: typeof timers.clearTimer;
  const setTimeout: typeof timers.setTimeout;
  const setInterval: typeof timers.setInterval;

  const console: Console;
  const window: Window;

  const addEventListener: typeof bridge.addEventListener;

  const Response: typeof FlyResponse;
  const Request: typeof FlyRequest;

  const fetch: typeof fetch_.fetch;

  // tslint:disable:variable-name
  let TextEncoder: typeof textEncoding.TextEncoder;
  let TextDecoder: typeof textEncoding.TextDecoder;
  // tslint:enable:variable-name

  let URL: typeof url.URL;
  let URLSearchParams: typeof url.URLSearchParams;

  let crypto: typeof crypto_.crypto;
  let cache: typeof cache_;

  // TODO: remove
  let flyData: typeof data;

  const resolv: typeof resolv_.resolv;
  const DNSClass: typeof dns.DNSClass;
  const DNSRecordType: typeof dns.DNSRecordType;
  const DNSMessageType: typeof dns.DNSMessageType;
  const DNSOpCode: typeof dns.DNSOpCode;
  const DNSResponseCode: typeof dns.DNSResponseCode;
}

// A reference to the global object.
export const window = globalEval("this");
window.window = window;

window.libfly = null;

window.setTimeout = timers.setTimeout;
window.setInterval = timers.setInterval;
window.Response = FlyResponse;
window.Request = FlyRequest;
// window.clearTimeout = timers.clearTimer;
// window.clearInterval = timers.clearTimer;

window.addEventListener = bridge.addEventListener;

window.console = new Console(libfly.print);
window.TextEncoder = textEncoding.TextEncoder;
window.TextDecoder = textEncoding.TextDecoder;
window.URL = url.URL;
window.URLSearchParams = url.URLSearchParams;

window.fetch = fetch_.fetch;
window.resolv = resolv_.resolv;
window.crypto = crypto_.crypto;
window.cache = cache_;

window.flyData = data;

window.DNSClass = dns.DNSClass;
window.DNSRecordType = dns.DNSRecordType;
window.DNSMessageType = dns.DNSMessageType;
window.DNSOpCode = dns.DNSOpCode;
window.DNSResponseCode = dns.DNSResponseCode;