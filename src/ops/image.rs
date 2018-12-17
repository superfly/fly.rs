// use libwebp_sys as webp;
// use image::{self, GenericImageView};



// #[cfg(test)]
// mod tests {
//     use super::*;
//     use image::{self, GenericImageView};
//     use libc::{c_float, c_int, c_void};
//     use libwebp_sys as webp;
//     use std::fs;
//     use std::io::{Read, Write};

//     #[test]
//     fn encode_webp() {
//         let mut contents = vec![];
//         let mut file = fs::File::open("balloon.jpg").unwrap();
//         file.read_to_end(&mut contents).unwrap();
//         let img = image::load_from_memory(contents.as_slice()).unwrap();
//         let width = img.width();
//         let height = img.height();
//         let stride = width * 3;
//         let lossless = false; // Set to true for lossless (Warning: CPU intensive/slow)
//         let quality: c_float = 75.0; // Quality level of the WebP image
//         let mut output: *mut u8 = std::ptr::null_mut();
//         let raw = img.raw_pixels();
//         unsafe {
//             let length: usize;
//             if lossless {
//                 length = webp::WebPEncodeLosslessRGB(
//                     raw.as_ptr(),
//                     width as c_int,
//                     height as c_int,
//                     stride as c_int,
//                     &mut output,
//                 );
//             } else {
//                 length = webp::WebPEncodeRGB(
//                     raw.as_ptr(),
//                     width as c_int,
//                     height as c_int,
//                     stride as c_int,
//                     quality,
//                     &mut output,
//                 );
//             }
//             let mut fout = fs::File::create("balloon.webp").unwrap();
//             fout.write_all(std::slice::from_raw_parts(output, length))
//                 .unwrap();
//             fout.flush();
//             webp::WebPFree(output as *mut c_void);
//         };
//     }
// }
