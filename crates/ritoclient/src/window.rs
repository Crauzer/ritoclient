//! Letting the Riot Client's window come forward.
//!
//! Windows only, and the module is gated rather than the function. Every caller
//! is inside a `cfg(windows)` block already, so a portable stub here has no
//! callers anywhere else and reads as dead code - which is what it is.

/// Let the Riot Client take the foreground when it processes our request,
/// matching what its own duplicate-instance path does. Best-effort: failing to
/// raise a window is not a reason to fail a launch.
pub fn allow_foreground() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

    // SAFETY: no pointers involved; ASFW_ANY is the documented "any process" value.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}
