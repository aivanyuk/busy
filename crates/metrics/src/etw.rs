//! Bytes sent and received per process, from a real-time event trace session on the
//! Microsoft-Windows-Kernel-Network provider (`OptIn::process_network`). Needs administrator rights: Performance
//! Log Users may start the session, but enabling this provider in it is still `ERROR_ACCESS_DENIED` (Win11 26300).
//! Events are consumed on a thread of the session's own (`busy-etw`), which `ProcessTrace` blocks.

use crate::util::Clock;
use busy_win::wide;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_NOT_ENOUGH_MEMORY, ERROR_SUCCESS, GetLastError, WIN32_ERROR,
};
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_RECORD,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2,
    OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, ProcessTrace, StartTraceW,
    TRACE_LEVEL_VERBOSE, WNODE_FLAG_TRACED_GUID,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::{GUID, PCWSTR, PWSTR};

/// Microsoft-Windows-Kernel-Network.
const PROVIDER: GUID = GUID::from_u128(0x7dd42a49_5329_4832_8dfd_43d979153a88);
/// KERNEL_NETWORK_KEYWORD_IPV4 | KERNEL_NETWORK_KEYWORD_IPV6.
const KEYWORDS: u64 = 0x10 | 0x20;
/// evntrace.h; not in the `windows` crate. `FlushTimer` is then in milliseconds, not seconds.
const EVENT_TRACE_USE_MS_FLUSH_TIMER: u32 = 0x10;
/// How often buffers are handed to the consumer, and so how late a sample sees traffic: well under the
/// shortest sampling interval (250 ms) evens out how much each sample gets.
const FLUSH_MS: u32 = 100;
/// `INVALID_PROCESSTRACE_HANDLE`.
const INVALID: u64 = u64::MAX;

/// Bytes moved by a TCP or UDP send or receive: (pid, bytes). Every such event's payload starts with the
/// process id and the size, both u32 (the provider's manifest: `(Get-WinEvent -ListProvider
/// Microsoft-Windows-Kernel-Network).Events`); other events (connect, disconnect, retransmit, reconnect) move nothing new.
fn transfer(id: u16, data: &[u8]) -> Option<(u32, u32)> {
    // TCP IPv4 send/receive, TCP IPv6, UDP IPv4, UDP IPv6.
    if !matches!(id, 10 | 11 | 26 | 27 | 42 | 43 | 58 | 59) {
        return None;
    }
    let pid = u32::from_le_bytes(*data.first_chunk::<4>()?);
    let size = u32::from_le_bytes(*data.get(4..)?.first_chunk::<4>()?);
    Some((pid, size))
}

/// Bytes per pid since the consumer last handed them over.
type Bytes = HashMap<u32, u64>;

/// Lives on the consumer thread until `ProcessTrace` returns; the trace's callbacks reach it through
/// `EVENT_RECORD::UserContext` and `EVENT_TRACE_LOGFILEW::Context`.
struct Consumer {
    /// What the callbacks add to, without a lock.
    own: Bytes,
    /// What the sampler drains. The consumer hands over only into an empty map, by swapping, so neither side
    /// allocates under the lock.
    shared: Arc<Mutex<Bytes>>,
}

unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a record valid for the call.
    let Some(rec) = (unsafe { rec.as_ref() }) else { return };
    // SAFETY: `UserContext` is the `Consumer` the trace was opened with, alive until `ProcessTrace` returns and
    // touched by this thread only.
    let Some(c) = (unsafe { rec.UserContext.cast::<Consumer>().as_mut() }) else { return };
    if rec.EventHeader.ProviderId != PROVIDER || rec.UserData.is_null() {
        return;
    }
    // SAFETY: ETW guarantees `UserDataLength` readable bytes at `UserData`.
    let data = unsafe { std::slice::from_raw_parts(rec.UserData.cast::<u8>(), rec.UserDataLength as usize) };
    if let Some((pid, size)) = transfer(rec.EventHeader.EventDescriptor.Id, data) {
        *c.own.entry(pid).or_default() += size as u64;
    }
}

/// Called after each buffer's events: hands what they added to the sampler if it has drained the last lot.
unsafe extern "system" fn on_buffer(log: *mut EVENT_TRACE_LOGFILEW) -> u32 {
    // SAFETY: as in `on_event`, `Context` is this thread's `Consumer`.
    if let Some(c) = unsafe { log.as_ref().and_then(|l| l.Context.cast::<Consumer>().as_mut()) } {
        let mut shared = c.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.is_empty() {
            std::mem::swap(&mut *shared, &mut c.own);
        }
    }
    // Keep going: the session ends when `Trace` stops it.
    1
}

/// A running session and its consumer. Dropping it stops the session, which ends `ProcessTrace` and the thread.
pub(crate) struct Trace {
    name: Vec<u16>,
    handle: CONTROLTRACE_HANDLE,
    shared: Arc<Mutex<Bytes>>,
    clock: Clock,
}

/// An `EVENT_TRACE_PROPERTIES` with room for the session name after it, as `StartTraceW`/`ControlTraceW` want;
/// u64s for its alignment.
fn properties(name: &[u16]) -> Vec<u64> {
    let size = size_of::<EVENT_TRACE_PROPERTIES>() + name.len() * 2;
    let mut buf = vec![0u64; size.div_ceil(8)];
    // SAFETY: `buf` is zeroed, 8-aligned and at least `size_of::<EVENT_TRACE_PROPERTIES>()` bytes.
    let p = unsafe { &mut *buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>() };
    p.Wnode.BufferSize = size as u32;
    p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    // Query performance counter timestamps: the cheapest, and we never read them.
    p.Wnode.ClientContext = 1;
    p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_USE_MS_FLUSH_TIMER;
    p.FlushTimer = FLUSH_MS;
    p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    buf
}

fn stop(handle: CONTROLTRACE_HANDLE, name: &[u16]) {
    let mut props = properties(name);
    // SAFETY: `props` is a properties block sized for the name; `name` is NUL-terminated.
    let _ =
        unsafe { ControlTraceW(handle, PCWSTR(name.as_ptr()), props.as_mut_ptr().cast(), EVENT_TRACE_CONTROL_STOP) };
}

impl Trace {
    /// Starts the session and its consumer thread. `ERROR_ACCESS_DENIED` without the rights to.
    pub(crate) fn start() -> Result<Self, WIN32_ERROR> {
        // Sessions are machine-wide: one per Windows session, as busy runs once per Windows session.
        let mut sid = 0;
        // SAFETY: plain out-parameter.
        let _ = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut sid) };
        let name = wide(&format!("busy network {sid}"));
        let mut handle = CONTROLTRACE_HANDLE::default();
        let mut rc = ERROR_ALREADY_EXISTS;
        // A session outlives the process that started it: one left by a busy that crashed is stopped and redone.
        for _ in 0..2 {
            let mut props = properties(&name);
            // SAFETY: `props` is a properties block sized for the name; `name` is NUL-terminated.
            rc = unsafe { StartTraceW(&mut handle, PCWSTR(name.as_ptr()), props.as_mut_ptr().cast()) };
            if rc != ERROR_ALREADY_EXISTS {
                break;
            }
            stop(CONTROLTRACE_HANDLE::default(), &name);
        }
        if rc != ERROR_SUCCESS {
            return Err(rc);
        }
        let trace = Self { name, handle, shared: Arc::default(), clock: Clock::default() };
        // SAFETY: `handle` is the session just started; the GUID outlives the call.
        let rc = unsafe {
            EnableTraceEx2(
                handle,
                &PROVIDER,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                TRACE_LEVEL_VERBOSE as u8,
                KEYWORDS,
                0,
                0,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(rc);
        }
        let consumer = Box::new(Consumer { own: Bytes::new(), shared: trace.shared.clone() });
        let context = Box::into_raw(consumer);
        let mut name = trace.name.clone();
        let mut log = EVENT_TRACE_LOGFILEW {
            LoggerName: PWSTR(name.as_mut_ptr()),
            BufferCallback: Some(on_buffer),
            Context: context.cast(),
            ..Default::default()
        };
        log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        log.Anonymous2.EventRecordCallback = Some(on_event);
        // SAFETY: `log` and the name it points at are valid for the call; `context` stays valid until the thread
        // that runs `ProcessTrace` frees it.
        let consume = unsafe { OpenTraceW(&mut log) };
        let reclaim = || {
            // SAFETY: `context` came from `Box::into_raw` and no callback can run any more.
            drop(unsafe { Box::from_raw(context) });
        };
        if consume.Value == INVALID {
            reclaim();
            // SAFETY: no other call since `OpenTraceW`.
            return Err(unsafe { GetLastError() });
        }
        let context = context as usize;
        let spawned = std::thread::Builder::new().name("busy-etw".into()).spawn(move || {
            // SAFETY: `consume` is the trace opened above; this thread is its only consumer. `ProcessTrace`
            // returns once the session stops (`Trace`'s drop).
            unsafe {
                let _ = ProcessTrace(&[consume], None, None);
                let _ = CloseTrace(consume);
            }
            // SAFETY: from `Box::into_raw` above; the callbacks that used it ran on this thread and are done.
            drop(unsafe { Box::from_raw(context as *mut Consumer) });
        });
        if spawned.is_err() {
            // SAFETY: never handed to `ProcessTrace`.
            let _ = unsafe { CloseTrace(consume) };
            reclaim();
            return Err(ERROR_NOT_ENOUGH_MEMORY);
        }
        Ok(trace)
    }

    /// Moves the bytes each process sent and received since the last call into `out` (pid -> bytes/s). Empty at
    /// first, as rates are on every source's first sample.
    pub(crate) fn drain(&mut self, out: &mut HashMap<u32, u64>) {
        let dt = self.clock.tick();
        // Only an empty map may be handed over: the consumer fills the shared one only while it is empty.
        out.clear();
        std::mem::swap(&mut *self.shared.lock().unwrap_or_else(PoisonError::into_inner), out);
        match dt {
            Some(dt) if dt > 0.0 => out.values_mut().for_each(|b| *b = (*b as f64 / dt) as u64),
            _ => out.clear(),
        }
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        stop(self.handle, &self.name);
    }
}

#[cfg(test)]
mod tests {
    use super::transfer;

    #[test]
    fn transfers_carry_pid_and_size() {
        let mut data = 1234u32.to_le_bytes().to_vec();
        data.extend(1500u32.to_le_bytes());
        data.extend([0xAB; 20]);
        for id in [10, 11, 26, 27, 42, 43, 58, 59] {
            assert_eq!(transfer(id, &data), Some((1234, 1500)), "{id}");
        }
        // Connect, disconnect, retransmit: nothing moved.
        for id in [12, 13, 14, 15, 16, 28, 29] {
            assert_eq!(transfer(id, &data), None, "{id}");
        }
    }

    #[test]
    fn short_payloads_are_dropped() {
        assert_eq!(transfer(10, &[1, 0, 0, 0, 2, 0, 0]), None);
        assert_eq!(transfer(10, &[1, 0, 0]), None);
        assert_eq!(transfer(10, &[]), None);
        assert_eq!(transfer(10, &[1, 0, 0, 0, 2, 0, 0, 0]), Some((1, 2)));
    }
}
