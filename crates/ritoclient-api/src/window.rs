//! Letting the Riot Client's window come forward.

/// Let the Riot Client take the foreground when it processes our request,
/// matching what its own duplicate-instance path does. Best-effort: failing to
/// raise a window is not a reason to fail a launch.
#[cfg(target_os = "windows")]
pub fn allow_foreground() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

    // SAFETY: no pointers involved; ASFW_ANY is the documented "any process" value.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}

#[cfg(not(target_os = "windows"))]
pub fn allow_foreground() {}
