use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{LazyLock, OnceLock};

use regex::bytes::{NoExpand, Regex as BytesRegex};

use crate::build::HOST_URL;
use crate::hook::Hook;
use crate::util;

const CURLOPT_SSL_VERIFYPEER: c_int = 64;
const CURLOPT_URL: c_int = 10002;
#[cfg(feature = "prod")]
const CURLOPT_PROXY: c_int = 10004;
const CURLE_BAD_FUNCTION_ARGUMENT: c_int = 43;
const CURLE_FAILED_INIT: c_int = 2;

type CurlVsetopt = unsafe extern "system" fn(*mut c_void, c_int, *mut c_void) -> c_int;

static CURL_VSETOPT: OnceLock<CurlVsetopt> = OnceLock::new();

static EPIC_GAMES_URL: LazyLock<BytesRegex> =
    LazyLock::new(|| BytesRegex::new(r"https://(.*)\.ol\.epicgames\.com").unwrap());

fn rewrite_url(url: &[u8]) -> Vec<u8> {
    let mut out = EPIC_GAMES_URL
        .replace_all(url, NoExpand(HOST_URL.as_bytes()))
        .into_owned();
    out.push(0);
    out
}

pub struct Curl {
    #[allow(dead_code)]
    hook: Hook,
}

impl Curl {
    pub fn new() -> Self {
        let vsetopt = util::find_pattern(
            &[
                0x48, 0x89, 0x5C, 0x24, 0x08, 0x48, 0x89, 0x6C, 0x24, 0x10, 0x48, 0x89, 0x74, 0x24,
                0x18, 0x57, 0x48, 0x83, 0xEC, 0x30, 0x33, 0xED, 0x49, 0x8B, 0xF0, 0x48, 0x8B, 0xD9,
            ],
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        );
        let _ = CURL_VSETOPT.set(unsafe { std::mem::transmute::<usize, CurlVsetopt>(vsetopt) });

        let easy_setopt = util::find_pattern(
            &[
                0x89, 0x54, 0x24, 0x10, 0x4C, 0x89, 0x44, 0x24, 0x18, 0x4C, 0x89, 0x4C, 0x24, 0x20,
                0x48, 0x83, 0xEC, 0x28, 0x48, 0x85, 0xC9, 0x75, 0x08, 0x8D, 0x41, 0x2B, 0x48, 0x83,
                0xC4, 0x28, 0xC3, 0x4C,
            ],
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        );

        let hook = match Hook::new(easy_setopt, curl_easy_setopt_detour as *const () as usize) {
            Ok(hook) => hook,
            Err(e) => util::fatal(&format!(
                "initializing hook for curl_easy_setopt failed: {e:?}"
            )),
        };

        Self { hook }
    }
}

unsafe extern "system" fn curl_easy_setopt_detour(
    ctx: *mut c_void,
    tag: c_int,
    arg: usize,
) -> c_int {
    if ctx.is_null() {
        return CURLE_BAD_FUNCTION_ARGUMENT;
    }

    let Some(&real) = CURL_VSETOPT.get() else {
        return CURLE_FAILED_INIT;
    };

    match tag {
        CURLOPT_SSL_VERIFYPEER => unsafe { call_vsetopt(real, ctx, tag, 0) }, // set to false

        CURLOPT_URL => {
            let url = unsafe { CStr::from_ptr(arg as *const c_char) }.to_bytes();
            let redirected = rewrite_url(url);
            unsafe { call_vsetopt(real, ctx, tag, redirected.as_ptr() as usize) }
        }

        #[cfg(feature = "prod")]
        CURLOPT_PROXY => unsafe { call_vsetopt(real, ctx, tag, c"".as_ptr() as usize) },

        _ => unsafe { call_vsetopt(real, ctx, tag, arg) },
    }
}

unsafe fn call_vsetopt(real: CurlVsetopt, ctx: *mut c_void, tag: c_int, value: usize) -> c_int {
    let mut slot = value;
    unsafe { real(ctx, tag, (&mut slot as *mut usize).cast()) }
}

#[cfg(test)]
mod tests {
    use super::{HOST_URL, rewrite_url};

    #[test]
    fn rewrite_url_redirects_epic_hosts_and_terminates() {
        let out = rewrite_url(
            b"https://fortnite-public-service-prod11.ol.epicgames.com/fortnite/api/calendar/v1/timeline?region=NA",
        );
        assert_eq!(
            out,
            format!("{HOST_URL}/fortnite/api/calendar/v1/timeline?region=NA\0").as_bytes()
        );
    }

    #[test]
    fn rewrite_url_passes_other_hosts_through_terminated() {
        let out = rewrite_url(b"https://example.com/some/path");
        assert_eq!(out, b"https://example.com/some/path\0");
    }
}
