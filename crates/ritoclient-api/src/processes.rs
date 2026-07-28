//! Which Riot processes are running.
//!
//! The remoting API cannot answer this: a lockfile survives a crash, and its pid
//! may since have been recycled onto something unrelated. So the process table
//! is the only source that can tell a live client from a stale record.
//!
//! # Games are not named here
//!
//! Only the Riot Client's own processes and Vanguard are named, because those
//! belong to the client this crate talks to. A *game's* executable is the
//! caller's to supply - see [`is_running`] - since which product is being
//! launched is not this crate's business.
//!
//! For "did the game I just launched actually start?", a process scan is the
//! blunt instrument. `/product-session/v1/external-sessions` is authoritative,
//! product-agnostic, and carries `exitCode`/`exitReason`, the patchline and the
//! version - and a launch already hands back the session id that keys it. Moving
//! to it is worth doing, and worth doing on its own rather than smuggled into a
//! refactor; the process scan stays as the fallback for when the Riot Client
//! exits while the game keeps running.

/// Lowercase basename of the Riot Client bootstrapper.
pub const RIOT_CLIENT_EXE: &str = "riotclientservices.exe";

/// Lowercase basenames of the Riot Client's own processes and Vanguard.
///
/// Deliberately no games: see the module docs.
pub const RIOT_PROCESS_NAMES: &[&str] = &[
    RIOT_CLIENT_EXE,
    "riotclientux.exe",
    "riotclientuxrender.exe",
    "riotclientcrashhandler.exe",
    "vgc.exe",
    "vgtray.exe",
];

/// Every running process as `(exe name, pid)`.
///
/// One snapshot walker answers every question anyone here has, so there is
/// deliberately no second one.
#[cfg(target_os = "windows")]
fn snapshot() -> Vec<(String, u32)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let mut out = Vec::new();
    // SAFETY: documented snapshot creation.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE || snap.is_null() {
        return out;
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: entry is correctly sized.
    if unsafe { Process32FirstW(snap, &mut entry) } == 0 {
        unsafe { CloseHandle(snap) };
        return out;
    }
    loop {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        out.push((
            String::from_utf16_lossy(&entry.szExeFile[..len]),
            entry.th32ProcessID,
        ));
        // SAFETY: entry is correctly sized; loop terminates on Process32NextW returning 0.
        if unsafe { Process32NextW(snap, &mut entry) } == 0 {
            break;
        }
    }
    // SAFETY: snap came from CreateToolhelp32Snapshot.
    unsafe { CloseHandle(snap) };
    out
}

#[cfg(not(target_os = "windows"))]
fn snapshot() -> Vec<(String, u32)> {
    Vec::new()
}

/// Running processes whose basename matches one of `names`, case-insensitively.
///
/// `names` are compared as basenames, so they should be spelled the way the
/// process table does - `"leagueclient.exe"`, not a path.
pub fn list_matching(names: &[&str]) -> Vec<(String, u32)> {
    snapshot()
        .into_iter()
        .filter(|(name, _)| names.iter().any(|wanted| name.eq_ignore_ascii_case(wanted)))
        .collect()
}

/// Every running Riot Client / Vanguard process as `(exe name, pid)`.
///
/// Games are not included - compose with [`list_matching`] to add them.
pub fn list_riot_processes() -> Vec<(String, u32)> {
    list_matching(RIOT_PROCESS_NAMES)
}

/// Whether a process with this basename is running.
pub fn is_running(exe_name: &str) -> bool {
    snapshot()
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(exe_name))
}

/// Whether this pid is still a Riot Client.
///
/// Both halves matter: pids get recycled, so "a process with this pid exists"
/// would happily accept an unrelated program that inherited it.
pub fn riot_client_alive(pid: u32) -> bool {
    snapshot().iter().any(|(name, running_pid)| {
        *running_pid == pid && name.eq_ignore_ascii_case(RIOT_CLIENT_EXE)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever this machine is running, a pid that cannot exist is not a Riot
    /// Client - and on a non-Windows runner nothing is.
    #[test]
    fn an_impossible_pid_is_never_a_live_client() {
        assert!(!riot_client_alive(0));
    }

    /// A game that is not named here must still be findable, which only works
    /// because the walk is unfiltered and the filtering happens after.
    #[test]
    fn a_name_outside_the_riot_set_is_still_answerable() {
        assert!(!is_running("definitely-not-a-real-process-8f3a.exe"));
        assert!(list_matching(&["definitely-not-a-real-process-8f3a.exe"]).is_empty());
    }

    /// The Riot set is the client's own processes plus Vanguard. A game leaking
    /// back into it would put product knowledge in a crate that must not have
    /// any.
    #[test]
    fn the_riot_process_set_names_no_games() {
        for name in RIOT_PROCESS_NAMES {
            assert!(
                !name.contains("league"),
                "{name} is a game; games belong to the caller"
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn nothing_is_running_off_windows() {
        assert!(list_riot_processes().is_empty());
        assert!(!is_running("leagueclient.exe"));
    }
}
