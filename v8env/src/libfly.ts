import { globalEval } from './global-eval'

type MessageCallback = (msg: Uint8Array, raw: Uint8Array) => void;
interface Libfly {
  recv(cb: MessageCallback): void;
  send(msg: ArrayBufferView, raw: ArrayBufferView | ArrayBufferLike): null | Uint8Array;
  print(lvl: number, msg: string): void;
  setGlobalErrorHandler: (
    handler: (
      message: string,
      source: string,
      line: number,
      col: number,
      error: Error
    ) => void
  ) => void;
  getNextStreamId(): number;
}

const window = globalEval("this");
export const libfly = window.libfly as Libfly;