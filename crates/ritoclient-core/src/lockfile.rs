//! The Riot Client's remoting lockfile.

use std::path::{Path, PathBuf};

/// A parsed `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`:
///
/// ```text
/// Riot Client:49768:49440:L4AV6MusE8gGdXU-pfCvEA:https
/// name        :pid  :port :password              :protocol
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Lockfile {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

/// Hand-written so the password can never reach a log through a `{:?}`. These
/// logs get attached to bug reports.
impl std::fmt::Debug for Lockfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lockfile")
            .field("name", &self.name)
            .field("pid", &self.pid)
            .field("port", &self.port)
            .field("password", &"<redacted>")
            .field("protocol", &self.protocol)
            .finish()
    }
}

impl Lockfile {
    /// Parse one lockfile line.
    ///
    /// Mirrors the client's own tolerance rather than demanding all five
    /// fields: three is enough, `password` defaults to empty and `protocol` to
    /// `https`.
    pub fn parse(line: &str) -> Option<Self> {
        let fields: Vec<&str> = line.trim().split(':').collect();
        if fields.len() < 3 {
            return None;
        }

        Some(Self {
            name: fields[0].to_string(),
            pid: fields[1].parse().ok()?,
            port: fields[2].parse().ok()?,
            password: fields.get(3).unwrap_or(&"").to_string(),
            protocol: fields.get(4).unwrap_or(&"https").to_string(),
        })
    }

    /// Read and parse the lockfile. Missing or unparseable is not an error -
    /// both mean the same thing to the caller as a dead pid does: cold start.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let lockfile = Self::parse(&text);
        if lockfile.is_none() {
            tracing::debug!("Unparseable Riot Client lockfile at {}", path.display());
        }
        lockfile
    }

    /// Base URL of the client's remoting server.
    pub fn base_url(&self) -> String {
        format!("{}://127.0.0.1:{}", self.protocol, self.port)
    }
}

/// The lockfile, but only when the client it names is genuinely alive.
///
/// A stale lockfile is the normal case after a crash, so its existence proves
/// nothing. Pids get recycled, hence the name check as well - "file missing",
/// "unparseable", "pid dead" and "wrong process" all mean the same thing to the
/// caller: there is no client to talk to.
///
/// **Re-read this before every request.** Waking a tray-idle client restarts its
/// remoting listener on a *new port* under the same pid, so a cached port is
/// dead within a second of the wake, and the pid is not a staleness signal for
/// it.
pub fn live_lockfile() -> Option<Lockfile> {
    let path = default_lockfile_path()?;
    let lockfile = Lockfile::load(&path)?;

    if !crate::processes::riot_client_alive(lockfile.pid) {
        tracing::debug!("Stale Riot Client lockfile (pid {} is gone)", lockfile.pid);
        return None;
    }
    Some(lockfile)
}

/// The default location of the Riot Client lockfile.
pub fn default_lockfile_path() -> Option<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        Path::new(&local_app_data)
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_canonical_five_field_line() {
        let lockfile = Lockfile::parse("Riot Client:49768:49440:L4AV6Mus:https").unwrap();
        assert_eq!(lockfile.name, "Riot Client");
        assert_eq!(lockfile.pid, 49768);
        assert_eq!(lockfile.port, 49440);
        assert_eq!(lockfile.password, "L4AV6Mus");
        assert_eq!(lockfile.protocol, "https");
        assert_eq!(lockfile.base_url(), "https://127.0.0.1:49440");
    }

    #[test]
    fn tolerates_a_trailing_newline() {
        let lockfile = Lockfile::parse("Riot Client:1:2:pw:https\n").unwrap();
        assert_eq!(lockfile.protocol, "https");
    }

    /// Below four fields the password is empty, below five the protocol
    /// defaults - matching the client instead of rejecting the line.
    #[test]
    fn short_lines_fall_back_to_defaults() {
        let three = Lockfile::parse("Riot Client:1:2").unwrap();
        assert_eq!(three.password, "");
        assert_eq!(three.protocol, "https");

        let four = Lockfile::parse("Riot Client:1:2:secret").unwrap();
        assert_eq!(four.password, "secret");
        assert_eq!(four.protocol, "https");
    }

    #[test]
    fn rejects_too_few_fields_and_garbage() {
        assert!(Lockfile::parse("").is_none());
        assert!(Lockfile::parse("Riot Client:1").is_none());
        assert!(Lockfile::parse("Riot Client:not-a-pid:2:pw:https").is_none());
        assert!(Lockfile::parse("Riot Client:1:not-a-port:pw:https").is_none());
    }

    /// A port above `u16::MAX` cannot be a real listener, and silently
    /// truncating it would send the POST somewhere arbitrary.
    #[test]
    fn rejects_an_out_of_range_port() {
        assert!(Lockfile::parse("Riot Client:1:70000:pw:https").is_none());
    }

    #[test]
    fn missing_file_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Lockfile::load(&dir.path().join("lockfile")).is_none());
    }

    #[test]
    fn loads_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lockfile");
        std::fs::write(&path, "Riot Client:49768:49440:pw:https").unwrap();
        assert_eq!(Lockfile::load(&path).unwrap().port, 49440);
    }

    /// The password must not survive a `{:?}` - these logs go into bug reports.
    #[test]
    fn debug_redacts_the_password() {
        let lockfile = Lockfile::parse("Riot Client:1:2:hunter2:https").unwrap();
        let rendered = format!("{lockfile:?}");
        assert!(!rendered.contains("hunter2"));
        assert!(rendered.contains("<redacted>"));
    }
}
