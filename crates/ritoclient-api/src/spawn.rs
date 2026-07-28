//! Cold-starting `RiotClientServices.exe`.

use std::path::Path;
use std::process::Command;

use crate::error::LauncherError;
use crate::launch::LaunchTarget;

/// Start the Riot Client with the product and patchline to launch.
///
/// Only those two arguments are passed. The bootstrapper computes `--app-root`,
/// `--data-root`, `--update-root`, `--log-root`, `--user-data-root` and
/// `--session-id` itself and gets them right; supplying our own would mean
/// owning the consequences of getting them wrong for no benefit.
///
/// Returns the pid of the spawned process. We never wait on it: the client
/// re-execs itself during self-update, so its exit code says nothing about
/// whether the launch worked.
pub fn cold_start(riot_client_exe: &Path, target: &LaunchTarget) -> Result<u32, LauncherError> {
    let mut command = Command::new(riot_client_exe);
    command
        .arg(format!("--launch-product={}", target.product_id))
        .arg(format!("--launch-patchline={}", target.patchline_id));

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
    tracing::info!(
        "Cold-started Riot Client (pid {pid}) for {}/{}",
        target.product_id,
        target.patchline_id
    );
    Ok(pid)
}
