use std::slice;
use std::thread;
use std::time::Duration;

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::GetCurrentProcess;

pub fn base_address() -> usize {
    unsafe { GetModuleHandleW(None).unwrap_or_default().0 as usize }
}

pub fn find_pattern(pattern: &[u8], mask: &str) -> usize {
    let (base, size) = module_range();

    loop {
        if let Some(found) = scan_pattern(base, size, pattern, mask) {
            return found;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn module_range() -> (usize, usize) {
    unsafe {
        let mut info = MODULEINFO::default();
        let process = GetCurrentProcess();
        let module = GetModuleHandleW(None).unwrap_or_default();
        GetModuleInformation(process, module, &mut info, size_of::<MODULEINFO>() as u32)
            .expect("GetModuleInformation failed");
        (base_address(), info.SizeOfImage as usize)
    }
}

fn scan_pattern(base: usize, size: usize, pattern: &[u8], mask: &str) -> Option<usize> {
    assert_eq!(
        pattern.len(),
        mask.len(),
        "pattern and mask must have the same length"
    );
    if pattern.is_empty() {
        return None;
    }

    let bytes = unsafe { slice::from_raw_parts(base as *const u8, size) };
    let window = size.saturating_sub(pattern.len());

    for i in 0..window {
        if mask_compare(&bytes[i..i + pattern.len()], pattern, mask) {
            return Some(base + i);
        }
    }
    None
}

fn mask_compare(buffer: &[u8], pattern: &[u8], mask: &str) -> bool {
    for (i, m) in mask.bytes().enumerate() {
        if m == b'x' && buffer[i] != pattern[i] {
            return false;
        }
    }
    true
}

pub fn fatal(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}
