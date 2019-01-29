import { sendAsync, streams, sendStreamChunks } from '../../bridge'
import * as fbs from "../../msg_generated";
import * as flatbuffers from "../../flatbuffers";
import { ReadableStream } from '@stardazed/streams';

export class Image {
    src: ReadableStream
    operations: Array<Image.Operation> = [];
    constructor(src: ReadableStream) {
        this.src = src;
    }

    webp(opts: Image.WebPOptions = {}) {
        this.operations.push({ type: Image.OperationType.WebPEncode, options: opts });
        return this
    }

    resize(opts: Image.ResizeOptions = {}) {
        this.operations.push({ type: Image.OperationType.Resize, options: opts });
        return this
    }

    transform(): Promise<ReadableStream> {
        const fbb = flatbuffers.createBuilder()

        let fbbTransforms = Array<number>();
        let i = 0;
        for (const op of this.operations) {
            if (op.type == Image.OperationType.WebPEncode) {
                let opts = <Image.WebPOptions>op.options;
                fbs.ImageWebPEncode.startImageWebPEncode(fbb);
                fbs.ImageWebPEncode.addLossless(fbb, !!opts.lossless);
                fbs.ImageWebPEncode.addNearLossless(fbb, !!opts.nearLossless);
                fbs.ImageWebPEncode.addQuality(fbb, opts.quality || 75);
                fbs.ImageWebPEncode.addAlphaQuality(fbb, opts.alphaQuality || 75);
                let fbbOpts = fbs.ImageWebPEncode.endImageWebPEncode(fbb);
                fbs.ImageTransform.startImageTransform(fbb);
                fbs.ImageTransform.addTransform(fbb, fbs.ImageTransformType.WebPEncode);
                fbs.ImageTransform.addOptionsType(fbb, fbs.ImageTransformOptions.ImageWebPEncode);
                fbs.ImageTransform.addOptions(fbb, fbbOpts);
                fbbTransforms[i++] = fbs.ImageTransform.endImageTransform(fbb);
            }
            else if (op.type === Image.OperationType.Resize) {
                let opts = <Image.ResizeOptions>op.options;
                fbs.ImageResize.startImageResize(fbb);
                fbs.ImageResize.addWidth(fbb, opts.width || 1);
                fbs.ImageResize.addHeight(fbb, opts.height || 1);
                fbs.ImageResize.addFilter(fbb, opts.filter || fbs.ImageSamplingFilter.Nearest);
                let fbbOpts = fbs.ImageResize.endImageResize(fbb);
                fbs.ImageTransform.startImageTransform(fbb);
                fbs.ImageTransform.addTransform(fbb, fbs.ImageTransformType.Resize);
                fbs.ImageTransform.addOptionsType(fbb, fbs.ImageTransformOptions.ImageResize);
                fbs.ImageTransform.addOptions(fbb, fbbOpts);
                fbbTransforms[i++] = fbs.ImageTransform.endImageTransform(fbb);
            }
        }
        const transforms = fbs.ImageApplyTransforms.createTransformsVector(fbb, fbbTransforms);
        fbs.ImageApplyTransforms.startImageApplyTransforms(fbb);
        fbs.ImageApplyTransforms.addTransforms(fbb, transforms);

        return sendAsync(fbb, fbs.Any.ImageApplyTransforms, fbs.ImageApplyTransforms.endImageApplyTransforms(fbb)).then(async base => {
            let msg = new fbs.ImageReady();
            base.msg(msg);

            await sendStreamChunks(msg.inId(), this.src);

            return new ReadableStream({
                start(controller) {
                    streams.set(msg.outId(), (chunkMsg: fbs.StreamChunk, raw: Uint8Array) => {
                        controller.enqueue(raw);
                        if (chunkMsg.done()) {
                            controller.close()
                            streams.delete(chunkMsg.id())
                        }
                    })
                }
            })
        })

    }
}

export namespace Image {
    export enum OperationType {
        WebPEncode,
        Resize,
    }

    type OperationOptions = WebPOptions | ResizeOptions;

    export interface Operation {
        type: OperationType,
        options: OperationOptions,
    }

    export interface WebPOptions {
        /** quality, integer 1-100, defaults to 80 */
        quality?: number,
        /** quality of alpha layer, integer 0-100, default to 100 */
        alphaQuality?: number,
        /** use lossless compression mode */
        lossless?: boolean,
        /** use near_lossless compression mode */
        nearLossless?: boolean,
        /** force WebP output, otherwise attempt to use input format, defaults to true */
        force?: boolean
    }

    export interface ResizeOptions {
        width?: number,
        height?: number,
        filter?: fbs.ImageSamplingFilter,
    }

    export const SamplingFilter = {
        Nearest: fbs.ImageSamplingFilter.Nearest,
        Triangle: fbs.ImageSamplingFilter.Triangle,
        CatmullRom: fbs.ImageSamplingFilter.CatmullRom,
        Gaussian: fbs.ImageSamplingFilter.Gaussian,
        Lanczos3: fbs.ImageSamplingFilter.Lanczos3,
    }
}