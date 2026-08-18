//! Orchestration that outlives a request: keeping the client's window put for
//! as long as a game runs.
//!
//! Like [`mod@crate::launch`], this sits above [`crate::namespaces`] rather than
//! among them. It polls, spawns a thread, and decides *when* to drive a route -
//! none of which is an endpoint wrapper's business.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ritoclient_api::namespaces::lifecycle::endpoints::Hide;

use crate::client::Client;

/// How long to keep watching for the game before giving up.
///
/// Generous because a cold start has to boot the client, possibly patch, and
/// wait for a login before the game appears. Giving up only costs the user a
/// window they can minimise themselves.
const GAME_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

/// Polled rather than pushed. `OnJsonApiEvent` would do this without polling,
/// but a WebSocket for one boolean is not worth the connection.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Once the game is up there is nothing to do but wait it out, and a session
/// runs for hours - so the walk of the process table slows down.
const SESSION_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// A running window-hiding watcher, and the way to call it off.
///
/// Dropping this does **not** stop the watcher. That is deliberate: the common
/// case is fire-and-forget for the length of a play session, and a guard that
/// cancelled on drop would make `hide_for_play_session(exe);` a no-op - the
/// quietest possible way to break a caller. Stopping is therefore something you
/// ask for.
///
/// Cheap to clone and safe to share; every clone stops the same watcher.
#[derive(Debug, Clone)]
pub struct SessionWatch {
    stopped: Arc<AtomicBool>,
}

impl SessionWatch {
    /// Call the watcher off.
    ///
    /// Takes effect at the next poll rather than immediately - the thread is
    /// usually asleep - so the window may be re-hidden once more after this
    /// returns. Idempotent.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    /// Whether the watcher is still running.
    ///
    /// Answers `false` once [`stop`](Self::stop) has been called *or* the
    /// watcher finished on its own - a caller cannot tell those apart, and
    /// nothing needs to.
    pub fn is_watching(&self) -> bool {
        !self.stopped.load(Ordering::Relaxed)
    }
}

/// How long the hide is re-asserted after the game exits.
///
/// Short on purpose. It has to outlast the teardown of the game's process tree,
/// which is where the client's own un-hide lands, and it has to stop well
/// before it starts fighting a player who opened the client from the tray.
const REHIDE_WINDOW: Duration = Duration::from_secs(10);

/// Keep the Riot Client hidden for one play session, on a background thread.
///
/// Returns immediately. The hide has to be deferred rather than done inline:
/// hiding the moment the launch request is accepted would hide the window while
/// the client is still working - mid-patch, or showing a login the user has to
/// complete - and on a cold start that gap is minutes, not seconds.
///
/// **Two hides, not one.** The first is the obvious one. The second exists
/// because the client un-hides *itself* when the game exits: Foundation's UX
/// command bus carries a `showUxIfHidden` flag, so the window we put away comes
/// back on its own the moment the session ends. One hide therefore lasts exactly
/// as long as the game does, which is not what "hide the Riot Client" means to
/// anyone who asked for it.
///
/// The second hide is re-asserted over a short window rather than fired once,
/// because the client's show lands somewhere inside the teardown of the game's
/// process tree and hiding an already-hidden window is a no-op. Then it stops:
/// past that point the only thing showing the window is the player, and a
/// watcher that kept re-hiding would be taking the tray icon away from them.
///
/// Entirely best-effort. Every failure is a log line, because the cost of
/// getting this wrong is a window that stayed visible.
///
/// The returned [`SessionWatch`] calls the whole thing off. Ignoring it leaves
/// the watcher running, which is the behaviour every caller wanted before there
/// was one to ignore.
///
/// ```no_run
/// use ritoclient::hide_for_play_session;
///
/// let watch = hide_for_play_session("LeagueClient.exe");
/// // ... the player asked us to stop managing their window:
/// watch.stop();
/// ```
pub fn hide_for_play_session(game_process: impl Into<String>) -> SessionWatch {
    let game_process = game_process.into();
    let stopped = Arc::new(AtomicBool::new(false));
    let watch = SessionWatch {
        stopped: Arc::clone(&stopped),
    };

    std::thread::spawn(move || {
        // One connection for the whole session. Safe to hold across the game
        // because it resolves the lockfile per request rather than at build
        // time, and this thread outlives several port changes.
        let Ok(client) = Client::new() else {
            tracing::debug!("Could not build a client; leaving the Riot Client visible");
            return;
        };

        if !wait_for_game(&game_process, &stopped) {
            tracing::debug!(
                "Game did not start within the hide window; leaving the client visible"
            );
            return;
        }
        tracing::info!("Game is up; hiding the Riot Client for this session");
        hide_now(&client);

        // No deadline: a session is as long as the player makes it.
        while crate::processes::is_running(&game_process) && !stopped.load(Ordering::Relaxed) {
            std::thread::sleep(SESSION_POLL_INTERVAL);
        }

        tracing::debug!("Game exited; keeping the Riot Client hidden through its own un-hide");
        let until = Instant::now() + REHIDE_WINDOW;
        while Instant::now() < until && !stopped.load(Ordering::Relaxed) {
            std::thread::sleep(POLL_INTERVAL);
            hide_now(&client);
        }

        // Whether it ran out or was called off, it is over - so the watch
        // reports what a caller would otherwise have to guess.
        stopped.store(true, Ordering::Relaxed);
    });

    watch
}

/// Wait for the game process, reporting whether it turned up in time.
///
/// Answers `false` when called off as well as when it times out: both mean
/// there is nothing left to do, and the caller does not act on the difference.
fn wait_for_game(game_process: &str, stopped: &AtomicBool) -> bool {
    let deadline = Instant::now() + GAME_WAIT_TIMEOUT;
    while Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
        if stopped.load(Ordering::Relaxed) {
            return false;
        }
        if crate::processes::is_running(game_process) {
            return true;
        }
    }
    false
}

/// Hide the client without announcing it.
///
/// Quiet because the re-assert loop calls this several times for one user-
/// visible event; [`hide_for_play_session`] logs the moments that mean
/// something instead. `ignore()` is the right finisher for exactly this shape
/// of caller: a hide that did not stick - transport failure or unhappy status
/// alike - is worth one debug line and nothing more.
fn hide_now(client: &Client) {
    if let Err(e) = client.endpoint(&Hide).ignore() {
        tracing::debug!("Could not hide the Riot Client: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The watch is the only handle on a thread that otherwise runs for hours.
    /// Stopping is idempotent because a caller cannot know whether the watcher
    /// already finished on its own.
    #[test]
    fn stopping_is_idempotent_and_visible() {
        let watch = SessionWatch {
            stopped: Arc::new(AtomicBool::new(false)),
        };
        assert!(watch.is_watching());

        watch.stop();
        assert!(!watch.is_watching());

        watch.stop();
        assert!(!watch.is_watching());
    }

    /// Every clone stops the same watcher - a host that hands one to its UI and
    /// keeps another must not end up with two half-controls.
    #[test]
    fn clones_share_one_watcher() {
        let watch = SessionWatch {
            stopped: Arc::new(AtomicBool::new(false)),
        };
        let handed_out = watch.clone();

        handed_out.stop();
        assert!(!watch.is_watching());
    }
}
