//! Starting a product through the Riot Client, and stopping one.
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
//! The routes differ only in how the client reaches a state where it can
//! answer. **`POST /product-launcher/...` is the only thing here that starts a
//! game**: the wake posts an empty argv and the cold start spawns bare, so
//! neither can launch on its own. That is a rule rather than an accident - the
//! alternatives all run through the client's startup middleware chain, where
//! the direct-launch gate can swallow a launch silently and no session id comes
//! back, and where they race the one launch we did ask for.
//!
//! [`close`] and the adopt step are the same route as the launch, at a
//! different verb: one resource, `POST` to start it, `DELETE` to stop it, `PUT`
//! to take ownership of a game that was already running when we arrived.
//!
//! This module is also where a launch's *judgement* lives - what a 404 from the
//! launcher means, how long a refusal is disbelieved, when to wake and when to
//! wait. The namespace handlers below it return statuses and decide nothing.
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchTarget {
    pub product_id: String,
    pub patchline_id: String,
}

impl LaunchTarget {
    /// Name a product and one of its patchlines.
    ///
    /// ```
    /// use ritoclient::LaunchTarget;
    /// use ritoclient::ids::{patchlines, products};
    ///
    /// let target = LaunchTarget::new(products::LEAGUE_OF_LEGENDS, patchlines::LIVE);
    /// assert_eq!(target.to_string(), "league_of_legends/live");
    /// ```
    pub fn new(product_id: impl Into<String>, patchline_id: impl Into<String>) -> Self {
        Self {
            product_id: product_id.into(),
            patchline_id: patchline_id.into(),
        }
    }
}

/// `league_of_legends/live` - the spelling the client's own logs use, which is
/// what makes it the right one for ours.
impl std::fmt::Display for LaunchTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.product_id, self.patchline_id)
    }
}

/// How the launch request was delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum LaunchRoute {
    /// Handed to an already-running Riot Client over its remoting API.
    ExistingClient,
    /// Cold-started `RiotClientServices.exe`, then launched through it once it
    /// was up. How the client got there, not a different kind of launch: the
    /// request that started the game is the same POST as
    /// [`LaunchRoute::ExistingClient`], and the outcome carries the same
    /// session id.
    ColdStart,
    /// The game was already up and the client already had a session for it, so
    /// no request was sent. An outcome rather than an error - what to do about
    /// it is the caller's business.
    AlreadyRunning,
    /// The game was already up and the client did **not** know about it, so we
    /// handed it the pid and it opened a session.
    ///
    /// The case the client itself describes as recovering a session for a
    /// product "that is already running, but Riot Client Services doesn't know
    /// about since it just started up" - a client restarted under a live game,
    /// or a game started outside it. Nothing was launched; what changed is that
    /// the outcome now carries a session id, so the caller can follow a game it
    /// did not start.
    Adopted,
}

/// The result of a successful launch request.
///
/// "Successful" means the Riot Client took the request, not that the game is up:
/// the client may still be updating itself, or waiting for the user to log in.
/// Two of the four routes did not ask for a launch at all - see [`LaunchRoute`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub route: LaunchRoute,
    /// Pid of the Riot Client - the one we spawned on a cold start, the one
    /// from the lockfile otherwise.
    pub riot_client_pid: Option<u32>,
    /// The session id the client minted, when it told us one.
    ///
    /// The key into `/product-session/v1/external-sessions`, so it is what a
    /// "did the game actually start?" check should follow rather than a process
    /// name: `ProductSessionHandler::external_session` turns it into a record
    /// with a phase and, once the game exits, a reason. `SessionExt` reads both.
    ///
    /// Present on every route that had one to give, including
    /// [`LaunchRoute::AlreadyRunning`] - a game we did not start still has a
    /// session, and the client will name it.
    pub session_id: Option<String>,
}

/// Whether a launch is possible right now, and why not if it isn't.
///
/// Never fails: an unanswerable question resolves to "can't launch" rather than
/// an error. Hosts that put this on the wire should map it to their own type -
/// this one names no product, and a UI's wording usually does.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
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
/// request, and catches a game that comes up during the wait for some reason of
/// its own: the user pressing Play, or a launch that was already in flight.
///
/// Returns as soon as the request is delivered. Callers that need to know the
/// game actually started must observe that separately - measured, the game
/// process appears ~3.8 s after the client accepts, on a client with nothing
/// to patch and a player already signed in.
///
/// Progress arrives on `observer` as [`LaunchStage`]s. They matter more here
/// than in most operations: a client booting from the tray can hold this call
/// for most of a minute, and silence for that long is indistinguishable from a
/// crash.
///
/// ```no_run
/// use ritoclient::{LaunchTarget, NullObserver, launch};
/// use ritoclient::ids::{patchlines, products};
///
/// let target = LaunchTarget::new(products::LEAGUE_OF_LEGENDS, patchlines::LIVE);
/// let outcome = launch(None, &target, "LeagueClient.exe", &NullObserver)?;
///
/// // The handle into `/product-session/v1/external-sessions`, when the client
/// // gave one. Absent is not a failure - see the field docs.
/// println!("{:?} {:?}", outcome.route, outcome.session_id);
/// # Ok::<(), ritoclient::LauncherError>(())
/// ```
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
        // Neither of these launched anything, so neither may report that it
        // did - a caller watching for `Launched` is watching for a game that
        // started because it asked.
        Ok(outcome)
            if matches!(
                outcome.route,
                LaunchRoute::AlreadyRunning | LaunchRoute::Adopted
            ) =>
        {
            LaunchStage::AlreadyRunning
        }
        Ok(_) => LaunchStage::Launched,
        Err(_) => LaunchStage::Error,
    };
    observer.on_progress(LaunchProgress::at(stage));

    result
}

/// Ask the Riot Client to close a product it launched.
///
/// The counterpart to [`launch`], and it answers on the same terms: success
/// means the client took the request, not that the game is gone. Measured, the
/// game is gone in under six seconds.
///
/// Needs a live client, because a closed one has nothing to close. Refusals
/// arrive as [`LauncherError::Refused`] with Riot's own code attached - a
/// product this client never launched is refused rather than quietly accepted.
///
/// ```no_run
/// use ritoclient::{LaunchTarget, close};
/// use ritoclient::ids::{patchlines, products};
///
/// close(&LaunchTarget::new(products::LEAGUE_OF_LEGENDS, patchlines::LIVE))?;
/// # Ok::<(), ritoclient::LauncherError>(())
/// ```
pub fn close(target: &LaunchTarget) -> Result<(), LauncherError> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = target;
        Err(LauncherError::UnsupportedPlatform)
    }

    #[cfg(target_os = "windows")]
    {
        use ritoclient_api::ClientExt;
        use ritoclient_core::client::Client;

        let client = Client::new()?;
        let response = client
            .product_launcher()
            .close(&target.product_id, &target.patchline_id)?;

        if !response.status().is_success() {
            return Err(refusal(&response, "close the game"));
        }

        tracing::info!("Riot Client closed {target}");
        Ok(())
    }
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

        if let Some(game_pid) = crate::processes::pid_of(game_process) {
            tracing::info!("{game_process} is already running; there is nothing to launch");
            return Ok(already_running(target, game_pid));
        }

        let installs_path = crate::installs::default_installs_path();
        let riot_client_exe = crate::installs::resolve_riot_client(&installs_path, product_root)?;
        tracing::debug!("Resolved Riot Client: {}", riot_client_exe.display());

        match crate::lockfile::live_lockfile() {
            Some(lockfile) => hand_off(lockfile.pid, target, game_process, observer),
            None => cold_start(&riot_client_exe, target, game_process, observer),
        }
    }
}

/// Boot a Riot Client because none was running, then launch through it.
///
/// Spawning is only half of it. The bootstrapper is started bare - see
/// [`crate::spawn::cold_start`] for why it carries no launch arguments - so
/// nothing launches until we ask, and asking means waiting out a boot that can
/// include a self-update. Returning at the spawn would report a launch that had
/// not been requested yet, let alone accepted.
#[cfg(target_os = "windows")]
fn cold_start(
    riot_client_exe: &Path,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    use ritoclient_core::client::Client;

    observer.on_progress(LaunchProgress::at(LaunchStage::ColdStart));
    let pid = crate::spawn::cold_start(riot_client_exe)?;

    // Built before the client exists, which costs nothing: the lockfile is read
    // per attempt, so until one appears every call answers `NoClient` and the
    // wait treats that as "not yet" like any other.
    let client = Client::new()?;

    wait_for_launcher(
        &client,
        LaunchRoute::ColdStart,
        pid,
        target,
        game_process,
        observer,
    )
}

/// Report a game that was already up, adopting it if the client is unaware.
///
/// The session lookup is what picks between the two routes, and it picks the
/// useful way round: a client that already has a session needs nothing from us,
/// and one that does not is exactly the case `PUT` exists for. Both are
/// successes - the caller asked for the game to be running, and it is.
///
/// Adopting is best-effort. A client that will not take the pid leaves the
/// outcome without a session id, which is what it carried before any of this
/// existed.
#[cfg(target_os = "windows")]
fn already_running(target: &LaunchTarget, game_pid: u32) -> LaunchOutcome {
    let riot_client_pid = crate::lockfile::live_lockfile().map(|l| l.pid);

    if let Some(session_id) = open_session_id(target) {
        return LaunchOutcome {
            route: LaunchRoute::AlreadyRunning,
            riot_client_pid,
            session_id: Some(session_id),
        };
    }

    match adopt(target, game_pid) {
        Some(session_id) => {
            tracing::info!("Adopted the running game as session {session_id}");
            LaunchOutcome {
                route: LaunchRoute::Adopted,
                riot_client_pid,
                session_id: Some(session_id),
            }
        }
        None => LaunchOutcome {
            route: LaunchRoute::AlreadyRunning,
            riot_client_pid,
            session_id: None,
        },
    }
}

/// Hand the client a running game's pid and take back the session id it mints.
///
/// The pid is the *game's*, never the Riot Client's - the client is being told
/// what to start tracking. Riot's binding types it `int32`, so a pid that does
/// not fit cannot be adopted; that is the client's limit rather than a cast to
/// paper over.
#[cfg(target_os = "windows")]
fn adopt(target: &LaunchTarget, game_pid: u32) -> Option<String> {
    use ritoclient_api::ClientExt;
    use ritoclient_core::client::Client;

    let pid = i32::try_from(game_pid).ok()?;
    let client = Client::new().ok()?;
    let response = client
        .product_launcher()
        .adopt(&target.product_id, &target.patchline_id, pid)
        .inspect_err(|e| tracing::debug!("Could not reach the client to adopt: {e}"))
        .ok()?;

    if !response.status().is_success() {
        tracing::debug!(
            "Riot Client would not adopt pid {game_pid}: HTTP {}",
            response.status()
        );
        return None;
    }
    response.json::<String>().ok()
}

/// The client's own id for a session it already has open on this target.
///
/// The process scan answers *whether* something is running; this answers *what*
/// the client thinks it is, which is the handle a caller needs to follow the
/// session afterwards. Best-effort by construction: a client that cannot answer
/// leaves the outcome without an id, which is what it carried before this
/// existed.
///
/// Ended sessions are skipped. The client keeps them around after the game
/// exits, so the newest matching record is not necessarily a live one, and
/// handing back a finished session as if it were the running game would be
/// worse than handing back nothing.
#[cfg(target_os = "windows")]
fn open_session_id(target: &LaunchTarget) -> Option<String> {
    use ritoclient_api::ClientExt;
    use ritoclient_core::client::Client;

    use crate::models_ext::SessionExt;

    let client = Client::new().ok()?;
    client
        .product_session()
        .external_sessions()?
        .into_iter()
        .find(|(_, session)| {
            session.product_id == target.product_id
                && session.patchline_id == target.patchline_id
                && !session.has_ended()
        })
        .map(|(id, _)| id)
}

/// What the launcher plugin has to say for itself right now.
///
/// Asking this is what stops the launch POST from doubling as a probe. "The
/// plugin is not there" and "the launch was refused" are different facts about
/// different things, and the status alone cannot separate them - both used to
/// arrive as a 404.
#[cfg(target_os = "windows")]
#[derive(Debug)]
enum Readiness {
    /// The plugin answered. `launch_pending` is its own report of whether a
    /// launch is already in flight, which is the guard against launching twice.
    Serving { launch_pending: bool },
    /// The route is absent, which on this client means the plugin is not
    /// registered: it is idling in the tray, where the argv handoff is the
    /// whole API surface. Nothing changes until it is woken.
    Idle,
    /// Registered but not serving yet, or nothing answered at all - a listener
    /// mid-restart, or a client that has not appeared. Both are "ask again";
    /// the reason is kept only to explain a timeout.
    NotYet { reason: String },
}

/// Ask the launcher plugin whether it can take a request.
///
/// `GET /product-launcher/v1/is-launch-request-pending` is the cheapest probe
/// the client offers: a plain GET that mutates nothing, on a route that exists
/// only once the plugin is mounted. The three-way [`Presence`] split is what
/// makes it trustworthy - `RESOURCE_NOT_FOUND` is a tray-idle client, whereas
/// `RPC_ERROR` is a handler that has not come up yet, and the two want
/// different responses. Its body then answers the double-launch question in the
/// same round trip.
///
/// [`Presence`]: ritoclient_core::client::Presence
#[cfg(target_os = "windows")]
fn readiness(client: &crate::client::Client) -> Readiness {
    use ritoclient_api::namespaces::product_launcher::endpoints::IsLaunchRequestPending;

    match client.endpoint(&IsLaunchRequestPending).send() {
        Ok(response) => readiness_of(&response),
        // No round trip, so there is nothing to read. A listener that is
        // restarting and a client that has not appeared look identical here,
        // and want the same thing: ask again.
        Err(e) => Readiness::NotYet {
            reason: e.to_string(),
        },
    }
}

/// Read a readiness probe's answer. Split out from [`readiness`] so the
/// classification can be tested against responses built by hand - it is the
/// judgement this module exists to make, and the only place a tray-idle client
/// is told apart from a busy one.
#[cfg(target_os = "windows")]
fn readiness_of(response: &crate::client::Response) -> Readiness {
    use ritoclient_core::client::Presence;

    let status = response.status();
    match Presence::from(response) {
        // Mounted and answering, but not with an answer: a plugin still
        // initialising 5xxs rather than 404ing.
        Presence::Serving if status.is_server_error() => Readiness::NotYet {
            reason: format!("HTTP {status}"),
        },
        Presence::Serving => Readiness::Serving {
            // Only a `true` justifies holding off. A body we cannot read is not
            // evidence of a launch in flight, and treating it as one would
            // stall until the deadline for the sake of a parse failure.
            launch_pending: response.json::<bool>().unwrap_or(false),
        },
        Presence::Absent => Readiness::Idle,
        Presence::Registered => Readiness::NotYet {
            reason: "the launcher plugin is registered but not serving yet".to_string(),
        },
    }
}

/// What a launch request found at the other end.
///
/// Two arms, because [`readiness`] is asked first: by the time this runs the
/// launcher is known to be serving, so the answer can only be launched or
/// refused. A refusal leaves as `Err` rather than a variant - it names a cause
/// the caller can act on, and this type is for the outcomes that do not.
#[cfg(target_os = "windows")]
#[derive(Debug)]
enum LaunchAttempt {
    /// The client launched the product. Carries the session id it minted, which
    /// is the key into `/product-session/v1/external-sessions`.
    Launched { session_id: Option<String> },
    /// No round trip completed, or the launcher stopped serving between the
    /// probe and the POST. Not a third meaning - core's rule showing through:
    /// there is no status to read, so there is nothing to classify. Ask again.
    NoAnswer { reason: String },
}

/// Ask the client to launch, and classify the answer.
///
/// Only ever called once [`readiness`] reports [`Readiness::Serving`], which is
/// what lets it be strict about statuses that used to mean "wait". A 404 here
/// is about the product or the patchline rather than the route, so it is
/// reported as the refusal it is instead of spending the whole boot budget on a
/// product id nobody has.
#[cfg(target_os = "windows")]
fn attempt_launch(
    client: &crate::client::Client,
    target: &LaunchTarget,
) -> Result<LaunchAttempt, LauncherError> {
    use ritoclient_api::ClientExt;
    use ritoclient_core::client::{Presence, StatusCode};

    // A POST that fails on our side may still have been accepted: the client
    // works through gates that outlast `LAUNCH_TIMEOUT`, and a timeout cancels
    // nothing at the far end. That is why this is a "not yet" rather than a
    // failure, and why the caller re-asks readiness before coming back here.
    let response = match client
        .product_launcher()
        .launch(&target.product_id, &target.patchline_id)
    {
        Ok(response) => response,
        Err(e) => {
            return Ok(LaunchAttempt::NoAnswer {
                reason: e.to_string(),
            });
        }
    };

    let status = response.status();

    // `RPC_ERROR` is the handler going away between the probe and this call - a
    // client that started restarting in the gap. `RESOURCE_NOT_FOUND` on a
    // bound path is about the product or patchline, so it falls through to be
    // refused rather than retried.
    if status == StatusCode::NOT_FOUND && Presence::from(&response) == Presence::Registered {
        return Ok(LaunchAttempt::NoAnswer {
            reason: format!("HTTP {status}, the launcher stopped serving"),
        });
    }

    // Mounted and unwell rather than mounted and refusing.
    if status.is_server_error() {
        return Ok(LaunchAttempt::NoAnswer {
            reason: format!("HTTP {status}"),
        });
    }

    // Anything else is the client answering: an unaccepted ToS, an ineligible
    // product, a locked patchline, a patchline that is not installed. Retrying
    // cannot change those, and the status alone cannot describe them - see
    // [`refusal`].
    if !status.is_success() {
        return Err(refusal(&response, "launch the game"));
    }

    // The body is a bare JSON string holding the session id. Losing it costs
    // us session tracking, not the launch, so a shape we don't recognise is
    // dropped rather than raised.
    let session_id = response.json::<String>().ok();
    tracing::info!("Riot Client launched {target} (session {session_id:?})");
    Ok(LaunchAttempt::Launched { session_id })
}

/// Turn a refused request into an error that names its cause.
///
/// `what` is the request in the imperative, for the log line only - the error
/// itself carries Riot's code and prose, which say more than any wording of
/// ours would.
///
/// Falls back to [`LauncherError::RiotClientUnreachable`] with the raw body when
/// the payload is not the client's error shape - a refusal we cannot explain is
/// still better reported verbatim than swallowed.
#[cfg(target_os = "windows")]
fn refusal(response: &crate::client::Response, what: &str) -> LauncherError {
    let Some(error) = response.riot_error() else {
        return LauncherError::RiotClientUnreachable {
            reason: format!("HTTP {}: {}", response.status(), response.body().trim()),
        };
    };

    let message = error.prose().to_string();
    tracing::warn!(
        "Riot Client refused to {what}: {} ({})",
        message,
        error.error_code
    );
    LauncherError::Refused {
        riot_error_code: error.error_code,
        message,
    }
}

/// Wake a tray-idle client by posting an empty argv over `new-args`.
///
/// **The array is empty on purpose.** `new-args` is the only route a tray-idle
/// client serves, which makes it the right wake and never the launch: its 204
/// means "arguments accepted" and says nothing about what the client does next.
/// On 135 a launch pair sent here launched nothing; on 136 the same body
/// launches, through the startup middleware chain the direct-launch gate sits
/// on, with no session id back and racing the `product-launcher` POST the wait
/// below is about to send. The difference is a cohort, so it is not a question
/// we can answer at runtime. An empty array gives every build nothing to act
/// on - see [`ritoclient_api::namespaces::app_args`].
///
/// **This is the call that moves the port.** Waking the client restarts its
/// remoting listener on a *new port* under the same pid, so from the second
/// attempt onwards any cached port names something nothing is listening on -
/// the retries would all fail against a client that had in fact just woken
/// up. The client re-reads the lockfile per attempt, which is what makes
/// retrying this particular call meaningful at all.
///
/// The loop is hand-rolled rather than a [`crate::retry::RetryPolicy`] because
/// [`crate::window::allow_foreground`] has to be re-asserted before *each*
/// attempt: the grant is consumed when a window takes the foreground, so
/// issuing it once ahead of a retry sequence would leave the later attempts
/// unable to raise the window.
#[cfg(target_os = "windows")]
fn wake(client: &crate::client::Client) -> Result<(), LauncherError> {
    use std::time::Duration;

    use ritoclient_api::namespaces::app_args::endpoints::NewArgs;
    use ritoclient_core::client::{RequestError, StatusCode};

    /// Waking is a cheap 204, so it does not get the launch budget - it is
    /// retried, and three attempts of the launch timeout would spend a minute
    /// before the wait for the client even starts.
    const WAKE_TIMEOUT: Duration = Duration::from_secs(5);

    /// The client may be mid-startup when we ask, so a refusal is worth a
    /// couple of retries before it counts as unreachable.
    const ATTEMPTS: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(250);

    let mut last_reason = String::from("no attempt was made");

    for attempt in 1..=ATTEMPTS {
        crate::window::allow_foreground();

        match client
            .endpoint(&NewArgs { args: &[] })
            .timeout(WAKE_TIMEOUT)
            .send()
        {
            Ok(response) if response.status() == StatusCode::NO_CONTENT => {
                tracing::debug!("Woke the Riot Client with new-args");
                return Ok(());
            }
            Ok(response) => last_reason = format!("HTTP {}", response.status()),
            Err(RequestError::NoClient) => {
                last_reason = "the Riot Client is no longer running".to_string();
            }
            Err(e) => last_reason = e.to_string(),
        }

        tracing::debug!("new-args attempt {attempt}/{ATTEMPTS} failed: {last_reason}");
        if attempt < ATTEMPTS {
            std::thread::sleep(RETRY_DELAY);
        }
    }

    Err(LauncherError::RiotClientUnreachable {
        reason: last_reason,
    })
}

/// Deliver a launch to a Riot Client that is already up.
///
/// Barely a route of its own any more: it opens the connection, names the pid
/// the outcome carries, warns on a target the client says it cannot launch, and
/// hands the same wait the cold start uses a client that happens to exist
/// already. The wait asks whether the launcher is serving before it asks for
/// anything, so a booted client launches on the first pass without waiting on
/// the poll interval.
#[cfg(target_os = "windows")]
fn hand_off(
    riot_client_pid: u32,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    use ritoclient_api::ClientExt;
    use ritoclient_core::client::Client;

    // One connection for the whole handoff, including the wait. It resolves the
    // lockfile per attempt, so it survives the port change that waking the
    // client causes - which is the entire reason a port is not cached here.
    let client = Client::new()?;

    if client
        .product_launcher()
        .is_eligible(&target.product_id, &target.patchline_id)
        == Some(false)
    {
        tracing::warn!("Riot Client reports {target} is not eligible to launch");
    }

    observer.on_progress(LaunchProgress::at(LaunchStage::HandingOff));

    wait_for_launcher(
        &client,
        LaunchRoute::ExistingClient,
        riot_client_pid,
        target,
        game_process,
        observer,
    )
}

/// Drive a client to a launch, waiting for as long as it takes it to get there.
///
/// One question per pass - is the game up, is the launcher serving, will it
/// take this - and every one of them can answer "not yet" without that being a
/// failure. The launch POST is sent only when the launcher has just said it is
/// serving and has no launch in flight, so its status means launched or
/// refused and nothing else.
///
/// Hand-rolled rather than a [`crate::retry::RetryPolicy`] because an iteration
/// is not a retry of one request: it asks several different things and tracks
/// how long a refusal has been running for. The policy type covers repeating *a
/// request*; this is repeating a decision.
///
/// `route` is carried through rather than inferred: the wait is identical
/// whether the client was already up or we just spawned it, and only the caller
/// knows which. It is how the outcome got here, not what happened here.
#[cfg(target_os = "windows")]
fn wait_for_launcher(
    client: &crate::client::Client,
    route: LaunchRoute,
    riot_client_pid: u32,
    target: &LaunchTarget,
    game_process: &str,
    observer: &dyn LaunchObserver,
) -> Result<LaunchOutcome, LauncherError> {
    use std::time::{Duration, Instant};

    /// Booting from the tray is tens of seconds on a cold disk, and the client
    /// may self-update on the way up. Overshooting costs a spinner the user can
    /// watch; undershooting reports a failure for a launch that then happens
    /// anyway, which is worse - the client turns up minutes later with no
    /// explanation attached to it.
    ///
    /// The budget covers a cold start too, where it runs from the spawn: that
    /// is the longest version of this wait and the one the number has to fit.
    ///
    /// The measured pieces are all far inside it - a wake is ~2.6 s, and the
    /// 5 s `ClientConfig` stall that shows up in launch traces is a cost of the
    /// lifecycle middleware chain, which the route we use does not walk. What
    /// the budget is really sized for is the self-update, which has no number.
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

    /// How long before a second wake is a nudge rather than an interruption.
    ///
    /// Waking restarts the remoting listener on a new port, so a wake per poll
    /// would keep resetting the thing the client is trying to finish booting
    /// behind. One nudge is the intent; this only exists so a wake lost to a
    /// listener that was already restarting is not the end of it.
    const WAKE_COOLDOWN: Duration = Duration::from_secs(10);

    let started = Instant::now();
    let mut last_reason = String::from("the Riot Client did not finish starting up in time");
    let mut refused_since: Option<Instant> = None;
    let mut last_wake: Option<Instant> = None;

    loop {
        // Nothing we sent can have started it - the wake is empty and the cold
        // start is bare - so this is the user pressing Play, or a launch that
        // was already in flight when we arrived. Either way the caller wanted
        // the game up and it is up.
        if crate::processes::is_running(game_process) {
            tracing::info!("{game_process} came up while we were waiting; nothing left to ask for");
            return Ok(LaunchOutcome {
                route,
                riot_client_pid: Some(riot_client_pid),
                session_id: open_session_id(target),
            });
        }

        match readiness(client) {
            Readiness::Serving {
                launch_pending: false,
            } => {
                // Read for the pid rather than the port: a client that restarted
                // during the wait comes back under a new one, and that is what
                // the outcome should report. The port is the connection's
                // business. It answered a moment ago, so a missing lockfile here
                // is a race, not a state - keep the pid we came in with.
                let pid = crate::lockfile::live_lockfile()
                    .map(|lockfile| lockfile.pid)
                    .unwrap_or(riot_client_pid);

                crate::window::allow_foreground();
                match attempt_launch(client, target) {
                    Ok(LaunchAttempt::Launched { session_id }) => {
                        return Ok(LaunchOutcome {
                            route,
                            riot_client_pid: Some(pid),
                            session_id,
                        });
                    }
                    Ok(LaunchAttempt::NoAnswer { reason }) => {
                        tracing::debug!("The launch request did not complete: {reason}");
                        last_reason = format!("the Riot Client never took the request: {reason}");
                    }
                    Err(refused @ LauncherError::Refused { .. }) => {
                        let since = *refused_since.get_or_insert_with(Instant::now);
                        if since.elapsed() >= REFUSAL_GRACE {
                            return Err(refused);
                        }
                        tracing::debug!("{refused}; still within the grace period");
                    }
                    Err(e) => return Err(e),
                }
            }

            // The client is working through the gates of a launch that was
            // already accepted - ours, timed out on our side, or someone
            // else's. Asking again would queue a second one; the POST is not
            // idempotent.
            Readiness::Serving {
                launch_pending: true,
            } => {
                tracing::debug!("A launch is already in flight; waiting rather than asking again");
            }

            Readiness::Idle => {
                if last_wake.is_none_or(|at| at.elapsed() >= WAKE_COOLDOWN) {
                    tracing::info!("The Riot Client is idling in the tray; waking it");
                    observer.on_progress(LaunchProgress::at(LaunchStage::WakingClient));
                    last_wake = Some(Instant::now());

                    // Best-effort. A client whose listener is restarting refuses
                    // the wake for the same reason it refused the probe, and
                    // this loop is what recovers from that - failing here would
                    // report the client unreachable while it was busy becoming
                    // reachable.
                    if let Err(e) = wake(client) {
                        tracing::debug!("Could not wake the Riot Client: {e}");
                    }
                }
                last_reason = "the Riot Client never came out of the tray".to_string();
            }

            // Transient failures are expected while the client reinitialises,
            // so only the deadline ends this loop - but the last reason is
            // kept, because it is the only description of *why* it ran out.
            Readiness::NotYet { reason } => {
                tracing::debug!("The launcher cannot take a request yet: {reason}");
                last_reason = format!("the Riot Client never became ready to launch: {reason}");
            }
        }

        if started.elapsed() >= BOOT_TIMEOUT {
            return Err(LauncherError::RiotClientUnreachable {
                reason: last_reason,
            });
        }

        // Emitted here rather than at the top of the pass, so a client that was
        // ready all along never reports a wait it did not have.
        observer.on_progress(LaunchProgress::waiting(
            started.elapsed().as_secs() as u32,
            BOOT_TIMEOUT.as_secs() as u32,
        ));
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Whether a launch is possible right now. Never fails.
///
/// ```
/// use ritoclient::availability;
///
/// let state = availability(None, "LeagueClient.exe");
/// if !state.can_launch {
///     // No Riot Client was resolved, so there is nothing to ask.
/// }
/// ```
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

    /// The refusal classification is pure parsing, but it lives beside the
    /// Windows-only wait it informs, so these run where it compiles.
    #[cfg(target_os = "windows")]
    mod refusals {
        use super::super::refusal;

        use assert_matches::assert_matches;
        use ritoclient_core::client::{Response, StatusCode};

        use crate::error::LauncherError;

        /// Verbatim from a live refusal, non-standard status and all. The
        /// status is the part that carries no information: 464 is not in the
        /// standard set, so a user shown "HTTP 464" learns nothing about what
        /// to do.
        const EULA_REFUSAL: &str = r#"{"errorCode":"eula_not_accepted","httpStatus":464,"implementationDetails":{},"message":"eula_not_accepted: Cannot run product 'league_of_legends' patchline 'live' because the player didn't accept EULA Terms of Service"}"#;

        fn response(status: u16, body: &str) -> Response {
            Response::new(StatusCode::from_u16(status), body.to_string())
        }

        #[test]
        fn a_refusal_keeps_riots_code_and_drops_its_prefix_from_the_prose() {
            assert_matches!(
                refusal(&response(464, EULA_REFUSAL), "launch the game"),
                LauncherError::Refused { riot_error_code, message } => {
                    assert_eq!(riot_error_code, "eula_not_accepted");
                    assert!(
                        message.starts_with("Cannot run product"),
                        "the code is a field, so the prose must not repeat it: {message}"
                    );
                }
            );
        }

        /// A refusal we cannot parse still has to reach the user - reporting it
        /// verbatim is worse than a tailored message and far better than
        /// nothing.
        #[test]
        fn an_unrecognised_body_falls_back_to_the_raw_text() {
            assert_matches!(
                refusal(&response(409, "patchline is locked"), "close the game"),
                LauncherError::RiotClientUnreachable { reason } => {
                    assert!(reason.contains("409"));
                    assert!(reason.contains("patchline is locked"));
                }
            );
        }
    }

    /// The readiness probe is the question the launch POST used to answer by
    /// accident. Its whole value is that the answers below mean different
    /// things, so they are pinned here.
    #[cfg(target_os = "windows")]
    mod readiness {
        use super::super::{Readiness, readiness_of};

        use assert_matches::assert_matches;
        use ritoclient_core::client::{Response, StatusCode};

        fn response(status: u16, body: &str) -> Response {
            Response::new(StatusCode::from_u16(status), body.to_string())
        }

        #[test]
        fn the_body_is_the_double_launch_guard() {
            assert_matches!(
                readiness_of(&response(200, "false")),
                Readiness::Serving {
                    launch_pending: false
                }
            );
            assert_matches!(
                readiness_of(&response(200, "true")),
                Readiness::Serving {
                    launch_pending: true
                }
            );
        }

        /// A launcher that is mounted but answering nonsense must not stall the
        /// wait: only a `true` is evidence of a launch in flight, and this is
        /// not one.
        #[test]
        fn an_unreadable_body_does_not_count_as_a_launch_in_flight() {
            assert_matches!(
                readiness_of(&response(200, "")),
                Readiness::Serving {
                    launch_pending: false
                }
            );
        }

        /// The split the whole design rests on. Both are 404s; one is a client
        /// that needs waking and the other is one that needs leaving alone.
        #[test]
        fn the_two_404s_mean_different_things() {
            assert_matches!(
                readiness_of(&response(
                    404,
                    r#"{"errorCode":"RESOURCE_NOT_FOUND","message":"no"}"#
                )),
                Readiness::Idle
            );
            assert_matches!(
                readiness_of(&response(
                    404,
                    r#"{"errorCode":"RPC_ERROR","message":"nope"}"#
                )),
                Readiness::NotYet { .. }
            );
        }

        /// A 404 with no payload to read is the tray-idle shape, because that
        /// is the state a bare 404 actually comes from - and waking a client
        /// that did not need it costs a restarted listener, not a launch.
        #[test]
        fn a_bare_404_reads_as_tray_idle() {
            assert_matches!(readiness_of(&response(404, "")), Readiness::Idle);
        }

        /// Mounted and unwell. Retrying is right; asking for a launch is not.
        #[test]
        fn a_server_error_is_not_a_serving_launcher() {
            assert_matches!(
                readiness_of(&response(503, "")),
                Readiness::NotYet { reason } => assert!(reason.contains("503"))
            );
        }
    }

    #[test]
    fn route_serializes_for_the_frontend() {
        let json = serde_json::to_value(LaunchRoute::ExistingClient).unwrap();
        assert_eq!(json, "EXISTING_CLIENT");

        let json = serde_json::to_value(LaunchRoute::AlreadyRunning).unwrap();
        assert_eq!(json, "ALREADY_RUNNING");

        let json = serde_json::to_value(LaunchRoute::Adopted).unwrap();
        assert_eq!(json, "ADOPTED");
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
