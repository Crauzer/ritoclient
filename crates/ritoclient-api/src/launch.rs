//! Starting a product through the Riot Client.
//!
//! We only *ask* for a launch - never spawning the game ourselves. A game's
//! argv carries an `rso_auth.authorization-key` blob only an authenticated Riot
//! Client can mint, so any design that starts it directly is wrong regardless of
//! how tempting the process tree makes it look.
//!
//! Two routes deliver the request, and picking the right one matters more than
//! it looks: Riot's process singleton is an exclusive lock on the lockfile, and
//! a second `RiotClientServices.exe` that fails to hand off its argv within
//! five seconds **terminates the running client** and takes the lock. So we
//! always probe the lockfile first and only cold-start when nothing is alive.
//!
//! Windows only. Everything else gets [`LauncherError::UnsupportedPlatform`].

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::LauncherError;
use crate::progress::{LaunchObserver, LaunchProgress, LaunchStage};

/// Which product and patchline to launch.
///
/// These are data, not an enum to invent: `league_of_legends` / `live` is what
/// the client's own product registry uses, `pbe` exists as a patchline, and
/// anything else should come from configuration rather than a guess.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchTarget {
    pub product_id: String,
    pub patchline_id: String,
}

/// How the launch request was delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LaunchRoute {
    /// Handed to an already-running Riot Client over its remoting API.
    ExistingClient,
    /// Cold-started `RiotClientServices.exe`.
    ColdStart,
    /// The game was already up, so no request was sent. An outcome rather than
    /// an error - what to do about it is the caller's business.
    AlreadyRunning,
}

/// The result of a successful launch request.
///
/// "Successful" means the Riot Client took the request, not that the game is up:
/// the client may still be updating itself, or waiting for the user to log in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub route: LaunchRoute,
    /// Pid of the Riot Client - the one we spawned on a cold start, the one
    /// from the lockfile otherwise.
    pub riot_client_pid: Option<u32>,
    /// The session id the client minted, when it told us one. This is the key
    /// into `/product-session/v1/external-sessions`, so it is what a future
    /// "did the game actually start?" check should follow rather than scanning
    /// for a process name.
    pub session_id: Option<String>,
}

/// Whether a launch is possible right now, and why not if it isn't.
///
/// Never fails: an unanswerable question resolves to "can't launch" rather than
/// an error. Hosts that put this on the wire should map it to their own type -
/// this one names no product, and a UI's wording usually does.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    /// Whether the platform supports launching and a Riot Client was resolved.
    pub can_launch: bool,
    /// The resolved `RiotClientServices.exe`, when one was found.
    pub riot_client_path: Option<String>,
    /// Whether a Riot Client is alive, i.e. whether a launch would take the
    /// handoff route rather than cold-starting.
    pub riot_client_running: bool,
    /// Whether the game is already up.
    pub game_running: bool,
}

/// Ask the Riot Client to launch a product.
///
/// `product_root` is the game's install root, used only to pick the Riot Client
/// that owns *this* install; `None` falls back to the machine's default client.
///
/// `game_process` is the executable to watch for - this crate names no products,
/// so the caller supplies it. It answers "is it already running?" before the
/// request and "did the wake arguments start it?" during the wait.
///
/// Returns as soon as the request is delivered. Callers that need to know the
/// game actually started must observe that separately.
///
/// Progress arrives on `observer` as [`LaunchStage`]s. They matter more here
/// than in most operations: a client booting from the tray can hold this call
/// for most of a minute, and silence for that long is indistinguishable from a
/// crash.
pub fn launch(
    product_root: Option<&Path>,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    let result = launch_inner(product_root, target, game_process, observer);

    // One terminal event per launch, on every exit path, so a listener always
    // gets told the request is over.
    let stage = match &result {
        Ok(outcome) if outcome.route == LaunchRoute::AlreadyRunning => LaunchStage::AlreadyRunning,
        Ok(_) => LaunchStage::Launched,
        Err(_) => LaunchStage::Error,
    };
    observer.on_progress(LaunchProgress::at(stage));

    result
}

fn launch_inner(
    product_root: Option<&Path>,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (product_root, target, game_process, observer);
        Err(LauncherError::UnsupportedPlatform)
    }

    #[cfg(target_os = "windows")]
    {
        observer.on_progress(LaunchProgress::at(LaunchStage::Resolving));

        if crate::processes::is_running(game_process) {
            tracing::info!("{game_process} is already running; there is nothing to launch");
            return Ok(LaunchOutcome {
                route: LaunchRoute::AlreadyRunning,
                riot_client_pid: crate::lockfile::live_lockfile().map(|l| l.pid),
                session_id: None,
            });
        }

        let installs_path = crate::installs::default_installs_path();
        let riot_client_exe = crate::installs::resolve_riot_client(&installs_path, product_root)?;
        tracing::debug!("Resolved Riot Client: {}", riot_client_exe.display());

        match crate::lockfile::live_lockfile() {
            Some(lockfile) => hand_off(lockfile.pid, target, game_process, observer),
            None => {
                observer.on_progress(LaunchProgress::at(LaunchStage::ColdStart));
                let pid = crate::spawn::cold_start(&riot_client_exe, target)?;
                Ok(LaunchOutcome {
                    route: LaunchRoute::ColdStart,
                    riot_client_pid: Some(pid),
                    session_id: None,
                })
            }
        }
    }
}

/// Deliver a launch to a Riot Client that is already up.
///
/// The happy path is one POST to the product-launcher. Anything short of that -
/// a tray-idle client that has not loaded the plugin, one still booting, one
/// whose remoting listener is restarting - is a "not yet": we nudge it and wait,
/// rather than reporting a failure for a client that is merely busy.
#[cfg(target_os = "windows")]
fn hand_off(
    riot_client_pid: u32,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    use crate::client::{Client, RequestError};
    use crate::namespaces::product_launcher::LaunchAttempt;

    // One connection for the whole handoff, including the wait. It resolves the
    // lockfile per attempt, so it survives the port change that waking the
    // client causes - which is the entire reason a port is not cached here.
    let client = Client::new().map_err(|e: RequestError| LauncherError::RiotClientUnreachable {
        reason: e.to_string(),
    })?;

    if client.product_launcher().is_eligible(target) == Some(false) {
        tracing::warn!(
            "Riot Client reports {}/{} is not eligible to launch",
            target.product_id,
            target.patchline_id
        );
    }

    observer.on_progress(LaunchProgress::at(LaunchStage::HandingOff));

    crate::window::allow_foreground();
    match client.product_launcher().launch(target) {
        Ok(LaunchAttempt::Launched { session_id }) => Ok(LaunchOutcome {
            route: LaunchRoute::ExistingClient,
            riot_client_pid: Some(riot_client_pid),
            session_id,
        }),
        Ok(LaunchAttempt::NotReady { reason }) => {
            tracing::info!("Riot Client cannot take the request yet ({reason}); waking it");
            observer.on_progress(LaunchProgress::at(LaunchStage::WakingClient));

            // Best-effort. A client whose listener is restarting refuses the
            // wake for the same reason it refused the launch, and the wait below
            // is what recovers from that - failing here would report the client
            // unreachable while it was busy becoming reachable.
            if let Err(e) = client.app_args().wake_with_launch_args(target) {
                tracing::debug!("Could not wake the Riot Client: {e}");
            }
            wait_for_launcher(&client, riot_client_pid, target, game_process, observer)
        }
        // A refusal is not necessarily the client's final answer - see the
        // grace period in [`wait_for_launcher`]. No wake: a client that can
        // refuse is awake enough, it just is not caught up yet.
        Err(refused @ LauncherError::LaunchRefused { .. }) => {
            tracing::info!("{refused}; giving the client a chance to catch up");
            wait_for_launcher(&client, riot_client_pid, target, game_process, observer)
        }
        Err(e) => Err(e),
    }
}

/// Poll a waking client until its product-launcher answers.
///
/// Hand-rolled rather than a [`crate::retry::RetryPolicy`] because an iteration
/// is not a retry of one request: it checks for the game, asks whether a launch
/// is already in flight, and only then tries again - and it tracks how long a
/// refusal has been running for. The policy type covers repeating *a request*;
/// this is repeating a decision.
#[cfg(target_os = "windows")]
fn wait_for_launcher(
    client: &crate::client::Client,
    riot_client_pid: u32,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    use std::time::{Duration, Instant};

    use crate::namespaces::product_launcher::LaunchAttempt;

    /// Booting from the tray is tens of seconds on a cold disk, and the client
    /// may self-update on the way up. Overshooting costs a spinner the user can
    /// watch; undershooting reports a failure for a launch that then happens
    /// anyway, which is worse - the client turns up minutes later with no
    /// explanation attached to it.
    const BOOT_TIMEOUT: Duration = Duration::from_secs(120);
    const POLL_INTERVAL: Duration = Duration::from_secs(1);

    /// How long a refusal is treated as the client not having caught up yet.
    ///
    /// A client that has just woken from the tray answers `eula_not_accepted`
    /// for a few seconds - its gates run against player state it has not
    /// fetched. That is indistinguishable from a player who genuinely has not
    /// accepted the terms, so a refusal is retried for a while and then
    /// reported *as itself*: waiting out the full boot budget would replace a
    /// message the player can act on with "the client never became ready".
    const REFUSAL_GRACE: Duration = Duration::from_secs(30);

    let started = Instant::now();
    let mut last_reason = String::from("the Riot Client did not finish starting up in time");
    let mut refused_since: Option<Instant> = None;

    loop {
        if started.elapsed() >= BOOT_TIMEOUT {
            return Err(LauncherError::RiotClientUnreachable {
                reason: last_reason,
            });
        }

        std::thread::sleep(POLL_INTERVAL);

        observer.on_progress(LaunchProgress::waiting(
            started.elapsed().as_secs() as u32,
            BOOT_TIMEOUT.as_secs() as u32,
        ));

        // The wake args are honoured on some builds, which starts the game
        // without us ever reaching the launcher. That is still a success.
        if crate::processes::is_running(game_process) {
            tracing::info!("{game_process} started from the wake arguments");
            return Ok(LaunchOutcome {
                route: LaunchRoute::ExistingClient,
                riot_client_pid: Some(riot_client_pid),
                session_id: None,
            });
        }

        // Read for the pid rather than the port: a client that restarted during
        // the wait comes back under a new one, and that is what the outcome
        // should report. The port is [`crate::client::Client`]'s business.
        let Some(lockfile) = crate::lockfile::live_lockfile() else {
            continue;
        };

        // An earlier POST can be accepted and still time out on our side - the
        // client works through gates that outlast our patience. Asking again
        // then would queue a *second* launch for a request already in flight.
        if client.product_launcher().is_launch_request_pending() == Some(true) {
            tracing::debug!("A launch is already in flight; waiting rather than asking again");
            continue;
        }

        // Transient failures are expected while the client reinitialises, so
        // only the deadline ends this loop - but the last one is kept, because
        // it is the only description of *why* the wait ran out.
        match client.product_launcher().launch(target) {
            Ok(LaunchAttempt::Launched { session_id }) => {
                return Ok(LaunchOutcome {
                    route: LaunchRoute::ExistingClient,
                    riot_client_pid: Some(lockfile.pid),
                    session_id,
                });
            }
            Ok(LaunchAttempt::NotReady { reason }) => {
                tracing::debug!("Riot Client still not ready: {reason}");
                last_reason = format!("the Riot Client never became ready to launch: {reason}");
            }
            Err(refused @ LauncherError::LaunchRefused { .. }) => {
                let since = *refused_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= REFUSAL_GRACE {
                    return Err(refused);
                }
                tracing::debug!("{refused}; still within the grace period");
            }
            Err(e) => return Err(e),
        }
    }
}

/// Whether a launch is possible right now. Never fails.
pub fn availability(product_root: Option<&Path>, game_process: &str) -> Availability {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (product_root, game_process);
        Availability::default()
    }

    #[cfg(target_os = "windows")]
    {
        let riot_client_path = crate::installs::resolve_riot_client(
            &crate::installs::default_installs_path(),
            product_root,
        )
        .ok();

        Availability {
            can_launch: riot_client_path.is_some(),
            riot_client_path: riot_client_path.map(|p| p.display().to_string()),
            riot_client_running: crate::lockfile::live_lockfile().is_some(),
            game_running: crate::processes::is_running(game_process),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the unsupported-platform path is reachable as a unit test: any call
    /// on Windows would resolve a real Riot Client and start a game. The Windows
    /// paths belong to `examples/launch.rs`.
    #[cfg(not(target_os = "windows"))]
    mod unsupported_platform {
        use super::*;

        use std::sync::Mutex;

        /// Captures the stages a launch reported, in order.
        #[derive(Default)]
        struct RecordingObserver(Mutex<Vec<LaunchStage>>);

        impl RecordingObserver {
            fn stages(&self) -> Vec<LaunchStage> {
                self.0.lock().unwrap().clone()
            }
        }

        impl LaunchObserver for RecordingObserver {
            fn on_progress(&self, progress: LaunchProgress) {
                self.0.lock().unwrap().push(progress.stage);
            }
        }

        /// Doubles as the coverage for the wrapper's terminal-event guarantee: a
        /// launch must announce that it is over on *every* exit path, or a
        /// listener leaves its spinner up forever.
        #[test]
        fn launching_is_windows_only() {
            let observer = RecordingObserver::default();
            let target = LaunchTarget {
                product_id: "league_of_legends".to_string(),
                patchline_id: "live".to_string(),
            };

            let error = launch(None, &target, "leagueclient.exe", &observer).unwrap_err();

            assert!(matches!(error, LauncherError::UnsupportedPlatform));
            assert_eq!(observer.stages(), vec![LaunchStage::Error]);
        }
    }

    #[test]
    fn route_serializes_for_the_frontend() {
        let json = serde_json::to_value(LaunchRoute::ExistingClient).unwrap();
        assert_eq!(json, "EXISTING_CLIENT");

        let json = serde_json::to_value(LaunchRoute::AlreadyRunning).unwrap();
        assert_eq!(json, "ALREADY_RUNNING");
    }

    #[test]
    fn outcome_serializes_camel_case() {
        let json = serde_json::to_value(LaunchOutcome {
            route: LaunchRoute::ColdStart,
            riot_client_pid: Some(4242),
            session_id: Some("irnZWC1kOMt".to_string()),
        })
        .unwrap();
        assert_eq!(json["route"], "COLD_START");
        assert_eq!(json["riotClientPid"], 4242);
        assert_eq!(json["sessionId"], "irnZWC1kOMt");
    }

    /// Nothing is launchable without a Riot Client, and the query must answer
    /// rather than fail - the button state depends on it.
    #[test]
    fn availability_defaults_to_not_launchable() {
        let availability = Availability::default();
        assert!(!availability.can_launch);
        assert!(!availability.game_running);
    }
}
