use std::ffi::{c_char, c_uint, c_void};

use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::Console::{AllocConsole, STD_OUTPUT_HANDLE, SetStdHandle};
use windows::core::w;

#[link(name = "ucrt")]
unsafe extern "C" {
    fn __acrt_iob_func(index: c_uint) -> *mut c_void;
    fn freopen(filename: *const c_char, mode: *const c_char, stream: *mut c_void) -> *mut c_void;
}

unsafe fn redirect_crt_stdout() {
    unsafe { freopen(c"CONOUT$".as_ptr(), c"w".as_ptr(), __acrt_iob_func(1)) };
}

pub fn init() {
    unsafe {
        let _ = AllocConsole();

        redirect_crt_stdout();

        if let Ok(conout) = CreateFileW(
            w!("CONOUT$"),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        ) {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, conout);
        }
    }
}
