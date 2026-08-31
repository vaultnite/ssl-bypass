use std::ffi::c_void;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH,
    EXCEPTION_POINTERS, RemoveVectoredExceptionHandler,
};
use windows::Win32::System::Memory::{
    MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_GUARD, PAGE_PROTECTION_FLAGS, VirtualProtect,
    VirtualQuery,
};

const EFLAGS_TRAP: u32 = 0x100;
const STATUS_GUARD_PAGE_VIOLATION: u32 = 0x8000_0001;
const STATUS_SINGLE_STEP: u32 = 0x8000_0004;

static TARGET: AtomicUsize = AtomicUsize::new(0);
static DETOUR: AtomicUsize = AtomicUsize::new(0);

pub struct Hook {
    handle: *mut c_void,
    target: usize,
    old_protect: PAGE_PROTECTION_FLAGS,
}

unsafe impl Send for Hook {}

impl Hook {
    pub fn new(target: usize, detour: usize) -> Result<Self, &'static str> {
        if is_same_page(target, detour) {
            return Err("hook target and detour share a page");
        }

        TARGET.store(target, Ordering::SeqCst);
        DETOUR.store(detour, Ordering::SeqCst);

        unsafe {
            let handle = AddVectoredExceptionHandler(1, Some(handler));
            if handle.is_null() {
                return Err("AddVectoredExceptionHandler failed");
            }

            let mut old_protect = PAGE_PROTECTION_FLAGS(0);
            if VirtualProtect(
                target as *const c_void,
                1,
                PAGE_EXECUTE_READ | PAGE_GUARD,
                &mut old_protect,
            )
            .is_err()
            {
                RemoveVectoredExceptionHandler(handle);
                return Err("VirtualProtect (PAGE_GUARD) failed");
            }

            Ok(Self {
                handle,
                target,
                old_protect,
            })
        }
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            let _ = VirtualProtect(
                self.target as *const c_void,
                1,
                self.old_protect,
                &mut PAGE_PROTECTION_FLAGS(0),
            );
            RemoveVectoredExceptionHandler(self.handle);
        }
    }
}

unsafe extern "system" fn handler(exception: *mut EXCEPTION_POINTERS) -> i32 {
    let exception = unsafe { &mut *exception };
    let record = unsafe { &mut *exception.ExceptionRecord };
    match record.ExceptionCode.0 as u32 {
        STATUS_GUARD_PAGE_VIOLATION => {
            let context = unsafe { &mut *exception.ContextRecord };
            if context.Rip == TARGET.load(Ordering::SeqCst) as u64 {
                context.Rip = DETOUR.load(Ordering::SeqCst) as u64;
            }
            context.EFlags |= EFLAGS_TRAP;
            EXCEPTION_CONTINUE_EXECUTION
        }
        STATUS_SINGLE_STEP => {
            let _ = unsafe {
                VirtualProtect(
                    TARGET.load(Ordering::SeqCst) as *const c_void,
                    1,
                    PAGE_EXECUTE_READ | PAGE_GUARD,
                    &mut PAGE_PROTECTION_FLAGS(0),
                )
            };
            EXCEPTION_CONTINUE_EXECUTION
        }
        _ => EXCEPTION_CONTINUE_SEARCH,
    }
}

fn is_same_page(first: usize, second: usize) -> bool {
    unsafe {
        let mut first_info: MEMORY_BASIC_INFORMATION = mem::zeroed();
        let mut second_info: MEMORY_BASIC_INFORMATION = mem::zeroed();
        if VirtualQuery(
            Some(first as *const c_void),
            &mut first_info,
            mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        ) == 0
        {
            return true;
        }
        if VirtualQuery(
            Some(second as *const c_void),
            &mut second_info,
            mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        ) == 0
        {
            return true;
        }
        first_info.BaseAddress == second_info.BaseAddress
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use windows::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, PAGE_EXECUTE_READWRITE, VirtualAlloc, VirtualFree,
    };

    use super::Hook;

    static DETOUR_CALLS: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn detour(arg: i64) -> i64 {
        DETOUR_CALLS.fetch_add(1, Ordering::SeqCst);
        arg * 100
    }

    #[test]
    fn guard_page_hook_redirects_and_rearms() {
        unsafe {
            let code = VirtualAlloc(None, 0x1000, MEM_COMMIT, PAGE_EXECUTE_READWRITE);
            assert!(!code.is_null(), "VirtualAlloc failed");

            let bytes = [0x48, 0x8B, 0xC1, 0x48, 0x83, 0xC0, 0x01, 0xC3];
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), code as *mut u8, bytes.len());

            let target = code as usize;
            let f: unsafe extern "system" fn(i64) -> i64 = std::mem::transmute(target);

            assert_eq!(f(41), 42); // unhooked

            let hook = Hook::new(target, detour as *const () as usize).expect("hook failed");
            assert_eq!(f(41), 4100);
            assert_eq!(DETOUR_CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(f(2), 200);
            assert_eq!(DETOUR_CALLS.load(Ordering::SeqCst), 2);

            drop(hook); // unhooked again
            assert_eq!(f(41), 42);

            let _ = VirtualFree(code, 0, MEM_RELEASE);
        }
    }
}
