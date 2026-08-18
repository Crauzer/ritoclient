//! Cold-starting `RiotClientServices.exe`.

use std::path::Path;
use std::process::Command;

use crate::error::LauncherError;

/// Start the Riot Client with no arguments at all.
///
/// **No arguments is deliberate**, and specifically not `--launch-product` /
/// `--launch-patchline`. Those make the client run its own launch through the
/// startup middleware chain, which is the path the direct-launch gate sits on -
/// so on an install inside that rollout the window opens and nothing launches.
/// Worse when it does work: that launch races the `product-launcher` POST the
/// caller is about to send, the POST is not idempotent, and the game starts
/// twice. A bare client boots to the window a user who opened it themselves
/// would see, and the POST is then literally the Play button.
///
/// The bootstrapper computes `--app-root`, `--data-root`, `--update-root`,
/// `--log-root`, `--user-data-root` and `--session-id` itself and gets them
/// right; supplying our own would mean owning the consequences of getting them
/// wrong for no benefit.
///
/// Returns the pid of the spawned process. We never wait on it: the client
/// re-execs itself during self-update, so its exit code says nothing about
/// whether the launch worked.
pub fn cold_start(riot_client_exe: &Path) -> Result<u32, LauncherError> {
    let mut command = Command::new(riot_client_exe);

    if let Some(parent) = riot_client_exe.parent() {
        command.current_dir(parent);
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW` - otherwise a console flashes on every launch.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command.spawn().map_err(|e| LauncherError::SpawnFailed {
        reason: e.to_string(),
    })?;

    let pid = child.id();
    tracing::info!("Cold-started Riot Client (pid {pid})");
    Ok(pid)
}
