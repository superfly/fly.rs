import * as fbs from "./msg_generated";
import * as flatbuffers from "./flatbuffers";
import { sendSync } from "./bridge";
import { assert } from "./util";

export function exit(code: number) {
  const builder = flatbuffers.createBuilder();
  fbs.OsExit.startOsExit(builder);
  fbs.OsExit.addCode(builder, code);
  const msg = fbs.OsExit.endOsExit(builder);
  const res = sendSync(builder, fbs.Any.OsExit, msg);
  assert(res == null);
}