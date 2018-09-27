#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

extern crate libc;
use libc::c_int;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::ptr;
use std::sync::Once;

pub enum Error {
    InitError,
}

pub struct Image(*mut VipsImage);

impl Image {
    pub fn from_file<T: Into<Vec<u8>>>(path: T) -> Result<Self, String> {
        let raw = match CString::new(path) {
            Ok(s) => unsafe { vips_image_new_from_file(s.as_ptr(), ptr::null() as *const c_int) },
            Err(e) => return Err(format!("{}", e)),
        };
        if raw.is_null() {
            return Err(get_last_error().unwrap());
        }
        Ok(Image(raw))
    }

    pub fn width(&self) -> Result<i32, String> {
        let i = unsafe { vips_image_get_width(self.as_raw()) };
        if let Some(err) = get_last_error() {
            return Err(err);
        }
        Ok(i)
    }

    fn as_raw(&self) -> *mut VipsImage {
        self.0
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.as_raw() as *mut _ as *mut c_void) };
    }
}

fn get_last_error() -> Option<String> {
    unsafe {
        let s = CStr::from_ptr(vips_error_buffer())
            .to_string_lossy()
            .into_owned();
        if s.is_empty() {
            return None;
        }
        vips_error_clear();
        Some(s)
    }
}

static VIPSINIT: Once = Once::new();

pub fn init(name: String) {
    VIPSINIT.call_once(|| unsafe {
        if vips_init(CString::new(name).unwrap().as_ptr()) != 0 {
            panic!("could not init libvips")
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate libc;
    use std::ffi::CString;
    #[test]
    fn it_works() {
        assert!(unsafe { vips_init(CString::new("fly").unwrap().as_ptr()) } == 0);
    }
}
