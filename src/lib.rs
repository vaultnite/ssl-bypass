mod build;
#[cfg(feature = "dev")]
mod console;
mod curl;
mod hook;
mod util;

use std::ffi::c_void;
use std::sync::Mutex;

use build::BUILD_ID;
use curl::Curl;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows::Win32::System::Threading::{CreateThread, THREAD_CREATION_FLAGS};

static RUNTIME: Mutex<Option<Curl>> = Mutex::new(None);

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_module: *mut c_void, reason: u32, _reserved: *mut c_void) -> bool {
    match reason {
        DLL_PROCESS_ATTACH => unsafe {
            if let Ok(thread) = CreateThread(
                None,
                0,
                Some(main_thread),
                None,
                THREAD_CREATION_FLAGS(0),
                None,
            ) {
                let _ = CloseHandle(thread);
            }
        },
        DLL_PROCESS_DETACH => {
            if let Some(curl) = RUNTIME.lock().unwrap().take() {
                drop(curl);
            }
        }
        _ => {}
    }

    true
}

unsafe extern "system" fn main_thread(_param: *mut c_void) -> u32 {
    match std::panic::catch_unwind(run) {
        Ok(()) => 0,
        Err(_) => std::process::exit(1),
    }
}

fn run() {
    #[cfg(feature = "dev")]
    console::init();

    println!("Vaultnite SSL Bypass ({})", BUILD_ID);
    println!("Built on: {}", env!("BUILD_TIMESTAMP"));

    *RUNTIME.lock().unwrap() = Some(Curl::new());
}
