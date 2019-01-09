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

pub enum OutputFormat {
    PNG,
    WEBP(WebPEncodeOptions),
    JPEG(u8),
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

pub fn encode_png(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();

    img.write_to(&mut buf, image::ImageOutputFormat::PNG)
        .map_err(|e| format!("{}", e))?;

    Ok(buf)
}

pub fn encode_jpeg(img: &image::DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();

    img.write_to(&mut buf, image::ImageOutputFormat::JPEG(quality))
        .map_err(|e| format!("{}", e))?;

    Ok(buf)
}

pub fn encode(img: &image::DynamicImage, format: OutputFormat) -> Result<Vec<u8>, String> {
    match format {
        OutputFormat::PNG => encode_png(&img),
        OutputFormat::WEBP(opts) => encode_webp(&img, opts),
        OutputFormat::JPEG(quality) => encode_jpeg(&img, quality),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_webp() {
        let input = image::open("./tests/images/baloon.jpg").expect("failed to load test image");

        let output = encode_webp(
            &input,
            WebPEncodeOptions {
                lossless: false,
                near_lossless: false,
                quality: 100.0,
                alpha_quality: 50.0,
            },
        )
        .expect("failed to encode image");

        let decoded = image::load_from_memory_with_format(&output, image::ImageFormat::WEBP)
            .expect("failed to decode output");

        assert_eq!(input.height(), decoded.height());
        assert_eq!(input.width(), decoded.width());
        assert_eq!(image::ColorType::Gray(8), decoded.color());
    }

    #[test]
    fn test_encode_png() {
        let input = image::open("./tests/images/baloon.jpg").expect("failed to load test image");

        let output = encode_png(&input).expect("failed to encode image");

        let decoded = image::load_from_memory_with_format(&output, image::ImageFormat::PNG)
            .expect("failed to decode output");

        assert_eq!(input.height(), decoded.height());
        assert_eq!(input.width(), decoded.width());
        assert_eq!(input.color(), decoded.color());
    }

    #[test]
    fn test_encode_jpeg() {
        let input = image::open("./tests/images/baloon.png").expect("failed to load test image");

        let output = encode_jpeg(&input, 70).expect("failed to encode image");

        let decoded = image::load_from_memory_with_format(&output, image::ImageFormat::JPEG)
            .expect("failed to decode output");

        assert_eq!(input.height(), decoded.height());
        assert_eq!(input.width(), decoded.width());
        assert_eq!(image::ColorType::RGB(8), decoded.color());
    }
}
