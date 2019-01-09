use image::{self, GenericImageView};
use libc::{c_float, c_int, c_void};
use libwebp_sys as webp;

#[derive(Debug)]
pub struct WebPEncodeOptions {
    pub lossless: bool,
    pub near_lossless: bool,
    pub quality: f32,
    pub alpha_quality: f32,
}

pub fn encode_webp(img: &image::DynamicImage, opts: WebPEncodeOptions) -> Result<Vec<u8>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode() {
        let input = image::open("./tests/images/picture.jpg").expect("failed to load test image");

        let output = encode_webp(
            &input,
            WebPEncodeOptions {
                lossless: false,
                near_lossless: false,
                quality: 100.0,
                alpha_quality: 100.0,
            },
        );
    }
}
