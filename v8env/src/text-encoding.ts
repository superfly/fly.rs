// @types/text-encoding relies on lib.dom.d.ts for some interfaces. We do not
// want to include lib.dom.d.ts (due to size) into fly's global type scope.
// Therefore this hack: add a few of the missing interfaces in
// @types/text-encoding to the global scope before importing.
declare global {
  type BufferSource = ArrayBufferView | ArrayBufferLike;

  interface TextDecodeOptions {
    stream?: boolean;
  }

  interface TextDecoderOptions {
    fatal?: boolean;
    ignoreBOM?: boolean;
  }

  interface TextDecoder {
    readonly encoding: string;
    readonly fatal: boolean;
    readonly ignoreBOM: boolean;
    decode(input?: BufferSource, options?: TextDecodeOptions): string;
  }
}

export { TextEncoder, TextDecoder } from "text-encoding";