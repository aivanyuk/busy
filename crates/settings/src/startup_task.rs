//! Autostart for a busy installed from the Microsoft Store: the package's startup task (`windows.startupTask`
//! in the package manifest). A packaged app's writes to the `Run` key land in its private copy of the registry
//! and start nothing.

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{E_ACCESSDENIED, E_FAIL};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{Error, HSTRING, Result};

/// The manifest's `TaskId`.
const TASK: &str = "busy";

/// Runs `f` on a thread of its own in the MTA, and waits for it. The caller (the settings worker) may be an STA
/// for the shell, where blocking on a WinRT async call is unsafe; a thread per call is cheap next to the call.
fn in_mta<T: Send>(f: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    std::thread::scope(|s| {
        s.spawn(|| {
            // SAFETY: plain call on a new thread; uninitialized below, after `f` has dropped its objects.
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
            let r = f();
            // SAFETY: pairs with the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
            r
        })
        .join()
        .unwrap_or_else(|_| Err(E_FAIL.into()))
    })
}

fn task() -> Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(TASK))?.join()
}

fn enabled(s: StartupTaskState) -> bool {
    s == StartupTaskState::Enabled || s == StartupTaskState::EnabledByPolicy
}

/// Why the task is still off after asking to turn it on, as the settings window shows it; None if it's on.
fn refusal(s: StartupTaskState) -> Option<&'static str> {
    match s {
        StartupTaskState::DisabledByUser => Some(
            "Start with Windows is turned off for busy in Windows Settings \u{2192} Apps \u{2192} Startup. Turn it \
             on there.",
        ),
        StartupTaskState::DisabledByPolicy => Some("Your organization keeps busy from starting with Windows."),
        s if enabled(s) => None,
        _ => Some("Windows left the startup task off."),
    }
}

pub(crate) fn is_enabled() -> bool {
    in_mta(|| task()?.State()).is_ok_and(enabled)
}

/// Turning it on can't override the user or a policy: the reason comes back as the error.
pub(crate) fn set(on: bool) -> Result<()> {
    in_mta(|| {
        let t = task()?;
        if !on {
            return t.Disable();
        }
        let state = t.RequestEnableAsync()?.join()?;
        refusal(state).map_or(Ok(()), |m| Err(Error::new(E_ACCESSDENIED, m)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_enabled_task_counts_and_refusals_say_why() {
        use StartupTaskState as S;
        assert!(enabled(S::Enabled) && enabled(S::EnabledByPolicy));
        assert!(!enabled(S::Disabled) && !enabled(S::DisabledByUser) && !enabled(S::DisabledByPolicy));
        assert_eq!(refusal(S::Enabled), None);
        assert_eq!(refusal(S::EnabledByPolicy), None);
        assert!(refusal(S::DisabledByUser).is_some_and(|m| m.contains("Apps \u{2192} Startup")));
        assert!(refusal(S::DisabledByPolicy).is_some());
        assert!(refusal(S::Disabled).is_some());
    }

    #[test]
    fn a_failing_call_is_an_error_not_a_panic() {
        assert!(in_mta(|| -> Result<()> { Err(E_ACCESSDENIED.into()) }).is_err());
        assert_eq!(in_mta(|| Ok(7)).ok(), Some(7));
    }
}
