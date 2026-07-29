//! `/riot-client-lifecycle/v1` - the Riot Client's own window state.
//!
//! Only the two window calls are wrapped, and the omissions are deliberate:
//!
//! - **`/quit` is not here.** A launched game holds a live remoting session
//!   with the client for its whole run (that is what `--riotclient-auth-token`
//!   and `--riotclient-app-port` are for), so quitting the client out from under
//!   a running game is not a tidier version of hiding it.
//! - **`/quit/switch-background-mode` is not here** either, though it looks like
//!   the honest answer to "put the client in the tray": background mode really
//!   does keep working, and even handles launches (`launchGameRequested` →
//!   *"Launch event handled in Background mode, modified the parameter to include
//!   direct launch args"*). Three things rule it out anyway. Its own description
//!   says it shows the games-running exit dialog when a game is up, which is
//!   exactly when we would be calling it. It sheds the plugin surface the game
//!   talks to for its whole session. And it leaves the client in the state whose
//!   only route is the argv handoff, so every later launch pays the wake-and-wait
//!   path. Hiding the window costs none of that.
//! - **There is no `/minimize`.** Probed and 404s; `hide` is the only thing the
//!   client offers. The window goes to the tray, not the taskbar.
//!
//! Hiding is reversible from the tray icon, and by [`LifecycleHandler::show`].
//! Keeping it hidden for the length of a play session is orchestration rather
//! than an endpoint - that is `session::hide_for_play_session` in the
//! `ritoclient` crate.

pub mod endpoints;
pub mod routes;

use ritoclient_core::client::{Client, RequestError, Response};

/// The `/riot-client-lifecycle/v1` namespace. Obtained from
/// [`ClientExt::lifecycle`](crate::ClientExt::lifecycle).
pub struct LifecycleHandler<'a> {
    client: &'a Client,
}

impl<'a> LifecycleHandler<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// `POST /riot-client-lifecycle/v1/hide` - "Hide the UX."
    ///
    /// Hides the window to the tray. The client keeps running, which is
    /// required: the game talks to it for the whole session.
    ///
    /// Idempotent: hiding an already-hidden window answers 204 like any other,
    /// which is what lets a session watcher re-assert it blind.
    pub fn hide(&self) -> Result<Response, RequestError> {
        self.client.endpoint(&endpoints::Hide).send()
    }

    /// `POST /riot-client-lifecycle/v1/show` - "Show the UX."
    pub fn show(&self) -> Result<Response, RequestError> {
        self.client.endpoint(&endpoints::Show).send()
    }
}
