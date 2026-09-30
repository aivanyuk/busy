//! The opt-in update check (`OptIn::update_check`): while it is on, one HTTPS GET of GitHub's latest release
//! at once and then every 24 hours, on a thread of its own. A newer release goes to the settings window's
//! About row (`busy_settings::release`); nothing is downloaded and nothing else changes. Errors are silent
//! and retried at the next check. While it is off, no request is made.

use crate::app::{WM_APP_RELEASE, main_hwnd};
use crate::sync::lock;
use busy_core::release::{self, LATEST_HOST, LATEST_PATH, Release};
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Networking::WinHttp::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{HSTRING, w};

const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Each of resolve, connect, send and receive.
const TIMEOUT_MS: i32 = 10_000;
/// GitHub's answer is a few kB; a larger one is not read further.
const MAX_BODY: usize = 1 << 20;

/// Whether the check is on, as the UI thread last set it; an answer arriving after it was turned off is dropped.
static ON: AtomicBool = AtomicBool::new(false);
/// The newer release found, handed to the UI thread on `WM_APP_RELEASE`.
static FOUND: Mutex<Option<Release>> = Mutex::new(None);

/// The check's thread. Dropping it ends the thread after a request in flight; nothing joins it, so exiting
/// never waits on the network.
pub(crate) struct Updates {
    tx: Option<Sender<bool>>,
}

impl Updates {
    pub(crate) fn start(on: bool) -> Self {
        let (tx, rx) = channel::<bool>();
        let spawned = std::thread::Builder::new()
            .name("busy-update".into())
            .spawn(move || {
                let mut on = false;
                loop {
                    if !on {
                        // Off: waits for it to be turned on, making no request.
                        match rx.recv() {
                            Ok(v) => on = v,
                            Err(_) => return,
                        }
                        continue;
                    }
                    if let Some(r) = check() {
                        *lock(&FOUND) = Some(r);
                        // SAFETY: PostMessageW only queues a message; a destroyed window's handle makes it fail.
                        let _ = unsafe { PostMessageW(Some(main_hwnd()), WM_APP_RELEASE, WPARAM(0), LPARAM(0)) };
                    }
                    // Until the next check, or until turned off (turned on again, it checks at once).
                    let next = Instant::now() + EVERY;
                    loop {
                        match rx.recv_timeout(next.saturating_duration_since(Instant::now())) {
                            Ok(false) => break on = false,
                            Ok(true) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
            })
            .is_ok();
        let u = Self { tx: spawned.then_some(tx) };
        u.set(on);
        u
    }

    /// Turns the check on or off (the config's `opt_in.update_check`); never blocks. Turning it off also
    /// takes a found release out of the settings window.
    pub(crate) fn set(&self, on: bool) {
        if ON.swap(on, Ordering::Relaxed) == on {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(on);
        }
        if !on {
            lock(&FOUND).take();
            busy_settings::release(None);
        }
    }
}

/// `WM_APP_RELEASE`: shows the newer release found, unless the check was turned off since.
pub(crate) fn deliver() {
    let found = lock(&FOUND).take();
    if ON.load(Ordering::Relaxed) {
        busy_settings::release(found.as_ref());
    }
}

/// Asks GitHub for the latest release; a newer one than this build, or None (also on any error).
fn check() -> Option<Release> {
    release::newer(env!("CARGO_PKG_VERSION"), &get(LATEST_PATH)?)
}

/// A WinHTTP handle, closed on drop.
struct Handle(*mut c_void);

impl Handle {
    fn new(h: *mut c_void) -> Option<Self> {
        (!h.is_null()).then_some(Self(h))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP returned, closed once; children are closed before their parents (drop order).
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// The body of `GET https://api.github.com` + `path` if it answered 200. Blocking, with `TIMEOUT_MS` on each
/// step; the system's proxy settings apply.
fn get(path: &str) -> Option<Vec<u8>> {
    let agent = HSTRING::from(format!("busy/{}", env!("CARGO_PKG_VERSION")));
    let (host, path) = (HSTRING::from(LATEST_HOST), HSTRING::from(path));
    let headers: Vec<u16> = "Accept: application/vnd.github+json
"
    .encode_utf16()
    .collect();
    // Each handle is owned by a `Handle` declared after its parent's, so it is closed first.
    // SAFETY: `agent` is NUL-terminated and outlives the call; no proxy strings.
    let session = Handle::new(unsafe { WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, None, None, 0) })?;
    // SAFETY: `session` is a live session handle.
    unsafe { WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS) }.ok()?;
    // SAFETY: `session` is live; `host` is NUL-terminated and outlives the call.
    let conn = Handle::new(unsafe { WinHttpConnect(session.0, &host, INTERNET_DEFAULT_HTTPS_PORT, 0) })?;
    // SAFETY: `conn` is live; `path` is NUL-terminated and outlives the call; a null accept-types list means none.
    let req = Handle::new(unsafe {
        WinHttpOpenRequest(conn.0, w!("GET"), &path, None, None, std::ptr::null(), WINHTTP_FLAG_SECURE)
    })?;
    // SAFETY: `req` is live; `headers` outlives the call (its length is passed with it); no request body.
    unsafe { WinHttpSendRequest(req.0, Some(&headers), None, 0, 0, 0) }.ok()?;
    // SAFETY: `req` is live and sent; the reserved pointer must be null.
    unsafe { WinHttpReceiveResponse(req.0, std::ptr::null_mut()) }.ok()?;
    let (mut status, mut len) = (0u32, size_of::<u32>() as u32);
    // SAFETY: `req` has a response; `status` is a writable u32 and `len` says so; no header index.
    unsafe {
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some((&raw mut status).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
    }
    .ok()?;
    if status != 200 {
        return None;
    }
    let mut body = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let mut read = 0u32;
        // SAFETY: `req` has a response; `buf` is writable for the length passed; `read` gets the count.
        unsafe { WinHttpReadData(req.0, buf.as_mut_ptr().cast(), buf.len() as u32, &mut read) }.ok()?;
        if read == 0 {
            return Some(body);
        }
        body.extend_from_slice(buf.get(..read as usize)?);
        if body.len() > MAX_BODY {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::get;

    /// Run by hand (`cargo test -p busy -- --ignored`): it makes real requests to api.github.com.
    #[test]
    #[ignore = "contacts api.github.com"]
    fn github_answers_a_latest_release() {
        let body = get("/repos/rust-lang/rust/releases/latest").expect("no answer");
        assert!(String::from_utf8_lossy(&body).contains("\"tag_name\""));
        assert_eq!(get("/repos/aivanyuk/busy/releases/no-such-thing"), None);
    }
}
