import * as fbs from "./msg_generated";

// @internal
export class FlyError<T extends fbs.ErrorKind> extends Error {
  constructor(readonly kind: T, msg: string) {
    super(msg);
    this.name = fbs.ErrorKind[kind];
  }
}

// @internal
export function maybeThrowError(base: fbs.Base): void {
  const err = maybeError(base);
  if (err != null) {
    throw err;
  }
}

export function maybeError(base: fbs.Base): null | FlyError<fbs.ErrorKind> {
  const kind = base.errorKind();
  if (kind === fbs.ErrorKind.NoError) {
    return null;
  } else {
    return new FlyError(kind, base.error()!);
  }
}