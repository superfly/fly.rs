use image::{self, GenericImageView};
use libwebp_sys as webp;

use libc::{c_float, c_int, c_void};

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::{JsBody, JsRuntime, Op};
use crate::utils::*;
use libfly::*;

use crate::NEXT_EVENT_ID;

use futures::{sync::mpsc, Future, Stream};

use std::sync::atomic::Ordering;

#[derive(Debug)]
struct WebPEncodeOptions {
    pub lossless: bool,
    pub near_lossless: bool,
    pub quality: f32,
    pub alpha_quality: f32,
}

enum ImageTransform {
    WebPEncode(WebPEncodeOptions),
}

pub fn op_image_transform(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_image_apply_transforms().unwrap();
    let transforms: Vec<ImageTransform> = match msg.transforms() {
        None => return Box::new(odd_future("image transforms required".to_string().into())),
        Some(t) => {
            let len = t.len();
            if len <= 0 {
                return Box::new(odd_future(
                    "at least 1 image transform required".to_string().into(),
                ));
            }
            (0..len)
                .map(|i| {
                    let item = t.get(i);
                    match item.transform() {
                        msg::ImageTransformType::ImageWebPEncode => {
                            let opts = item.options_as_image_web_pencode().unwrap();
                            ImageTransform::WebPEncode(WebPEncodeOptions {
                                lossless: opts.lossless(),
                                near_lossless: opts.near_lossless(),
                                alpha_quality: opts.alpha_quality(),
                                quality: opts.quality(),
                            })
                        }
                    }
                })
                .collect()
        }
    };

    let in_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;
    let out_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let rt = ptr.to_runtime();

    let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
    {
        rt.streams.lock().unwrap().insert(in_id, sender);
    }

    rt.spawn(
        recver
            .map_err(|e| error!("error cache set stream! {:?}", e))
            .concat2()
            .and_then(
                move |chunks: Vec<u8>| match image::load_from_memory(chunks.as_slice()) {
                    Err(e) => Err(error!("error loading image from memory: {}", e)),
                    Ok(img) => {
                        for t in transforms {
                            match t {
                                ImageTransform::WebPEncode(opts) => {
                                    let v = encode_webp(&img, opts).unwrap();
                                    send_body_stream(ptr, out_id, JsBody::Static(v));
                                }
                            };
                        }
                        Ok(())
                    }
                },
            ),
    );

    let builder = &mut FlatBufferBuilder::new();
    let msg = msg::ImageReady::create(builder, &msg::ImageReadyArgs { in_id, out_id });
    ok_future(serialize_response(
        cmd_id,
        builder,
        msg::BaseArgs {
            msg: Some(msg.as_union_value()),
            msg_type: msg::Any::ImageReady,
            ..Default::default()
        },
    ))
}

fn encode_webp(img: &image::DynamicImage, opts: WebPEncodeOptions) -> Result<Vec<u8>, String> {
    let width = img.width();
    let height = img.height();
    let stride = width * 3;

    let lossless = opts.lossless; // Set to true for lossless (Warning: CPU intensive/slow)
    let _near_lossless = opts.near_lossless;
    let _alpha_quality = opts.alpha_quality;
    let quality: c_float = opts.quality as c_float; // Quality level of the WebP image
    let mut output: *mut u8 = std::ptr::null_mut();
    let raw = img.raw_pixels();
    unsafe {
        let length: usize;
        if lossless {
            length = webp::WebPEncodeLosslessRGB(
                raw.as_ptr(),
                width as c_int,
                height as c_int,
                stride as c_int,
                &mut output,
            );
        } else {
            length = webp::WebPEncodeRGB(
                raw.as_ptr(),
                width as c_int,
                height as c_int,
                stride as c_int,
                quality,
                &mut output,
            );
        }
        let v = std::slice::from_raw_parts(output, length);
        webp::WebPFree(output as *mut c_void);
        Ok(v.to_vec())
    }
}
