use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::OnceLock;

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

static MATCH_HOSTS: &[&str] = &["ol.epicgames.com", "ol.epicgames.net", "epicgames.dev"];

fn is_matched_host(host: &str) -> bool {
    MATCH_HOSTS
        .iter()
        .any(|candidate| host == *candidate || host.ends_with(&format!(".{candidate}")))
}

fn terminated(url: &[u8]) -> Vec<u8> {
    let mut out = url.to_vec();
    out.push(0);
    out
}

fn rewrite_host(url: &[u8]) -> Vec<u8> {
    let bare = url
        .strip_prefix(b"https://")
        .or_else(|| url.strip_prefix(b"http://"));

    let Some(bare) = bare else {
        return terminated(url);
    };
    let host_end = bare
        .iter()
        .position(|b| b"/:?#".contains(b))
        .unwrap_or(bare.len());
    let (host, tail) = bare.split_at(host_end);

    match std::str::from_utf8(host) {
        Ok(host) if is_matched_host(host) => [HOST_URL.as_bytes(), tail, &[0]].concat(),
        _ => terminated(url),
    }
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
        CURLOPT_SSL_VERIFYPEER => unsafe { call_vsetopt(real, ctx, tag, 0) }, // disable ssl pinning

        CURLOPT_URL => {
            let url = unsafe { CStr::from_ptr(arg as *const c_char) }.to_bytes();
            let redirected = rewrite_host(url);
            unsafe { call_vsetopt(real, ctx, tag, redirected.as_ptr() as usize) }
        } // rewrite hosts from epic servers to ours

        #[cfg(feature = "prod")]
        CURLOPT_PROXY => unsafe { call_vsetopt(real, ctx, tag, c"".as_ptr() as usize) }, // disables system proxy respect; artifact from original aurora.runtime

        _ => unsafe { call_vsetopt(real, ctx, tag, arg) },
    }
}

unsafe fn call_vsetopt(real: CurlVsetopt, ctx: *mut c_void, tag: c_int, value: usize) -> c_int {
    let mut slot = value;
    unsafe { real(ctx, tag, (&mut slot as *mut usize).cast()) }
}

#[cfg(test)]
mod tests {
    use super::{HOST_URL, rewrite_host};

    #[test]
    fn rewrite_host_redirects_epic_host_and_terminates() {
        let out = rewrite_host(
            b"https://fortnite-public-service-prod11.ol.epicgames.com/fortnite/api/calendar/v1/timeline?region=NA",
        );
        assert_eq!(
            out,
            format!("{HOST_URL}/fortnite/api/calendar/v1/timeline?region=NA\0").as_bytes()
        );
    }

    #[test]
    fn rewrite_host_redirects_epic_dev_host_and_terminates() {
        let out = rewrite_host(
            b"https://fortnite-public-service-prod11.ol.epicgames.net/fortnite/api/calendar/v1/timeline?region=NA",
        );
        assert_eq!(
            out,
            format!("{HOST_URL}/fortnite/api/calendar/v1/timeline?region=NA\0").as_bytes()
        );
    }

    #[test]
    fn rewrite_host_redirects_eos_host_and_terminates() {
        let out = rewrite_host(b"https://api.epicgames.dev/auth/v1/oauth/token?junk_key=TEST");
        assert_eq!(
            out,
            format!("{HOST_URL}/auth/v1/oauth/token?junk_key=TEST\0").as_bytes()
        );
    }

    #[test]
    fn rewrite_host_passes_other_hosts_through_terminated() {
        let out = rewrite_host(b"https://example.com/some/path");
        assert_eq!(out, b"https://example.com/some/path\0");
    }
}
