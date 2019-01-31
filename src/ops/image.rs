use image::{self, GenericImageView};
use libwebp_sys as webp;

use libc::{c_float, c_int, c_void};

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::js::*;
use crate::runtime::Runtime;
use crate::utils::*;
use libfly::*;

use crate::get_next_stream_id;

use futures::{sync::mpsc, Future, Stream};
use std::{fmt, fmt::Display};

#[derive(Debug)]
struct WebPEncodeOptions {
    pub lossless: bool,
    pub near_lossless: bool,
    pub quality: f32,
    pub alpha_quality: f32,
}

struct ResizeOptions {
    pub width: u32,
    pub height: u32,
    pub filter: image::FilterType,
}

enum ImageTransform {
    WebPEncode(WebPEncodeOptions),
    Resize(ResizeOptions),
}

impl Display for ImageTransform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ImageTransform::Resize(_) => "Resize",
                ImageTransform::WebPEncode(_) => "WebP",
            }
        )
    }
}

pub fn op_image_transform(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
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
                        msg::ImageTransformType::WebPEncode => {
                            let opts = item.options_as_image_web_pencode().unwrap();
                            ImageTransform::WebPEncode(WebPEncodeOptions {
                                lossless: opts.lossless(),
                                near_lossless: opts.near_lossless(),
                                alpha_quality: opts.alpha_quality(),
                                quality: opts.quality(),
                            })
                        }
                        msg::ImageTransformType::Resize => {
                            let opts = item.options_as_image_resize().unwrap();
                            ImageTransform::Resize(ResizeOptions {
                                width: opts.width(),
                                height: opts.height(),
                                filter: match opts.filter() {
                                    msg::ImageSamplingFilter::Nearest => image::FilterType::Nearest,
                                    msg::ImageSamplingFilter::Triangle => {
                                        image::FilterType::Triangle
                                    }
                                    msg::ImageSamplingFilter::CatmullRom => {
                                        image::FilterType::CatmullRom
                                    }
                                    msg::ImageSamplingFilter::Gaussian => {
                                        image::FilterType::Gaussian
                                    }
                                    msg::ImageSamplingFilter::Lanczos3 => {
                                        image::FilterType::Lanczos3
                                    }
                                },
                            })
                        }
                    }
                })
                .collect()
        }
    };

    let in_id = get_next_stream_id();
    let out_id = get_next_stream_id();

    let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
    {
        rt.streams.lock().unwrap().insert(in_id, sender);
    }

    let ptr = rt.ptr;

    rt.spawn(
        recver
            .map_err(|e| error!("error cache set stream! {:?}", e))
            .concat2()
            .and_then(
                move |chunks: Vec<u8>| match image::load_from_memory(chunks.as_slice()) {
                    Err(e) => Err(error!("error loading image from memory: {}", e)),
                    Ok(mut img) => {
                        let mut encode: Option<&ImageTransform> = None;
                        for t in transforms.iter() {
                            debug!("Applying image transform: {}", t);
                            match t {
                                ImageTransform::Resize(opts) => {
                                    img = img.resize(opts.width, opts.height, opts.filter);
                                }
                                ImageTransform::WebPEncode(_) => {
                                    encode = Some(t);
                                }
                            };
                        }

                        if let Some(enc) = encode {
                            match enc {
                                ImageTransform::WebPEncode(opts) => {
                                    let v = encode_webp(&img, opts).unwrap();
                                    send_body_stream(ptr, out_id, JsBody::Static(v));
                                    return Ok(());
                                }
                                _ => {}
                            }
                        }

                        send_body_stream(ptr, out_id, JsBody::Static(img.raw_pixels()));
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

fn encode_webp(img: &image::DynamicImage, opts: &WebPEncodeOptions) -> Result<Vec<u8>, String> {
    let width = img.width();
    let height = img.height();
    let stride = width * 3;

    debug!("WEBP WIDTH: {}, HEIGHT: {}", width, height);

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
