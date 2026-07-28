//! Orchestration that outlives a request: keeping the client's window put for
//! as long as a game runs.
//!
//! Like [`mod@crate::launch`], this sits above [`crate::namespaces`] rather than
//! among them. It polls, spawns a thread, and decides *when* to drive a route -
//! none of which is an endpoint wrapper's business.

use std::time::{Duration, Instant};

use crate::client::Client;
use crate::namespaces::lifecycle;

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
pub fn hide_for_play_session(game_process: impl Into<String>) {
    let game_process = game_process.into();

    std::thread::spawn(move || {
        // One connection for the whole session. Safe to hold across the game
        // because it resolves the lockfile per request rather than at build
        // time, and this thread outlives several port changes.
        let Ok(client) = Client::new() else {
            tracing::debug!("Could not build a client; leaving the Riot Client visible");
            return;
        };

        if !wait_for_game(&game_process) {
            tracing::debug!(
                "Game did not start within the hide window; leaving the client visible"
            );
            return;
        }
        tracing::info!("Game is up; hiding the Riot Client for this session");
        hide_now(&client);

        // No deadline: a session is as long as the player makes it.
        while crate::processes::is_running(&game_process) {
            std::thread::sleep(SESSION_POLL_INTERVAL);
        }

        tracing::debug!("Game exited; keeping the Riot Client hidden through its own un-hide");
        let until = Instant::now() + REHIDE_WINDOW;
        while Instant::now() < until {
            std::thread::sleep(POLL_INTERVAL);
            hide_now(&client);
        }
    });
}

/// Wait for the game process, reporting whether it turned up in time.
fn wait_for_game(game_process: &str) -> bool {
    let deadline = Instant::now() + GAME_WAIT_TIMEOUT;
    while Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
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
/// something instead.
fn hide_now(client: &Client) {
    if let Err(e) = lifecycle::post(client, lifecycle::routes::HIDE) {
        tracing::debug!("Could not hide the Riot Client: {e}");
    }
}
