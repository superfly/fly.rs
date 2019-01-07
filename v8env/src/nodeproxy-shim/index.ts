import * as bridge from "./bridge";

export function installNodeProxyShim(target: any) {
  target.bridge = bridge;
}