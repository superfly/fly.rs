use image::{GenericImageView};

// # resize options
//   # dimensions
//     resize by axis
//     200x => 200 on the x axis, scale y
//     200y => 200 on the y axis, scale x
//     200x100 => fit inside 200x100 preserve aspect ratio
//     - only if larger
//     - only if smaller
//     - force into dimensions breaking aspect ratio
//   # filter

pub enum RotateOptions {
  RotateZero,
  Rotate90,
  Rotate180,
  Rotate270
}

pub fn rotate(img: &image::DynamicImage, opts: RotateOptions) -> Result<DynamicImage, String> {
  match opts {
    RotateOptions::RotateZero => Ok(img),
    RotateOptions::Rotate90 => Ok(img.rotate90()),
    RotateOptions::Rotate180 => Ok(img.rotate180()),
    RotateOptions::Rotate270 => Ok(img.rotate270()),
  }
}

pub fn resize(img: &image::DynamicImage) -> Result<(), String> {
  img.res
}