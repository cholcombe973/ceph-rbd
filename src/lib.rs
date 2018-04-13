extern crate ceph;
extern crate libc;
#[macro_use]
extern crate log;
pub mod ffi;
pub mod rbd;

use self::ceph::error::RadosResult;
use self::libc::{c_char, c_int, strerror_r};

pub(crate) fn get_error(n: c_int) -> RadosResult<String> {
    let mut buf = vec![0u8; 256];
    unsafe {
        strerror_r(n, buf.as_mut_ptr() as *mut c_char, buf.len());
    }
    buf = buf.iter().take_while(|&x| x != &0u8).cloned().collect();
    Ok(String::from_utf8_lossy(&buf).into_owned())
}
