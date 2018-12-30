export function assert(cond: boolean, msg = "assert") {
  if (!cond) {
    throw Error(msg);
  }
}

export function assertNotNull<T>(value: T | null, msg = "assert not null"): value is T {
  if (value === null) {
    throw Error(msg);
  }
  return true;
}

export function assertNotUndef<T>(value: T | undefined, msg = "assert not undefined"): value is T {
  if (typeof value === "undefined") {
    throw Error(msg);
  }
  return true;
}

export function assertNotNullOrUndef<T>(value: T | null | undefined, msg = "assert not null or undefined"): value is T {
  if (!assertNotUndef(value, msg) || !assertNotNull(value, msg)) {
    throw Error(msg);
  }
  return true;
}