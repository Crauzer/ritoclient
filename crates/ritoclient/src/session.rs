//! Orchestration that outlives a request: following a session to its end, and
//! keeping the client's window put for as long as a game runs.
//!
//! Like [`mod@crate::launch`], this sits above [`crate::namespaces`] rather than
//! among them. It polls, spawns a thread, and decides *when* to drive a route -
//! none of which is an endpoint wrapper's business.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ritoclient_api::ClientExt;
use ritoclient_api::models::product_session::Session;
use ritoclient_api::namespaces::lifecycle::endpoints::Hide;

use crate::client::Client;
use crate::models_ext::{SessionExt, SessionPhase, TerminationReason};
use crate::retry::RetryPolicy;

/// How long to keep watching for the game before giving up.
///
/// Generous because a cold start has to boot the client, possibly patch, and
/// wait for a login before the game appears. Giving up only costs the user a
/// window they can minimise themselves.
const GAME_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

/// Polled rather than pushed. `OnJsonApiEvent` would do this without polling,
/// but a WebSocket for a poll this cheap is not worth the connection.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Once the game is up there is nothing to do but wait it out, and a session
/// runs for hours - so the polling slows down.
const SESSION_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// A running watcher, and the way to call it off.
///
/// Both watchers in this module hand one back: [`watch_session`] and
/// [`hide_for_play_session`].
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
    /// usually asleep - so one more poll's worth of work can land after this
    /// returns: a window re-hidden once more, or one more event on a session
    /// observer. Idempotent.
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

/// What a watched session did.
///
/// Reported in order by [`watch_session`]: [`Opened`](Self::Opened) once, then
/// any number of [`PhaseChanged`](Self::PhaseChanged) and
/// [`GameRunning`](Self::GameRunning), then exactly one of
/// [`Ended`](Self::Ended) or [`Lost`](Self::Lost), after which the watcher is
/// done.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SessionEvent {
    /// The client opened the session, at the phase it opened in.
    Opened {
        phase: SessionPhase,
        version: String,
        /// Whether the game process was already up when the session opened.
        ///
        /// False for the ordinary launch - the client mints the session first
        /// and the process follows a few seconds later - and true for a session
        /// adopted or recovered under a game that was already running.
        game_running: bool,
    },
    /// The phase moved.
    ///
    /// This is what the *match* is doing, not whether the game is up: see
    /// [`SessionPhase`] for why those are different questions, and
    /// [`GameRunning`](Self::GameRunning) for the other one.
    PhaseChanged {
        from: SessionPhase,
        to: SessionPhase,
    },
    /// The game process appeared, or went away.
    ///
    /// The answer to "is the game up", which the phase does not give: a client
    /// sitting on its own home screen reports phase `None` with the process
    /// very much alive. A host that acts on the game existing - applying mods,
    /// hiding a window, saying so in a status bar - wants this one.
    ///
    /// Fires only on a change, so the state at the start of the watch rides on
    /// [`Opened`](Self::Opened) instead.
    GameRunning { running: bool },
    /// The session ended, and the client said why.
    ///
    /// Both numbers are the client's own. `exit_code` is meaningful only once
    /// the session has ended, which is exactly when this fires.
    Ended {
        exit_code: i64,
        reason: TerminationReason,
    },
    /// The client stopped answering for this session and the game process is
    /// gone. Separate from [`Ended`](Self::Ended) because no reason arrived - a
    /// host that reports one must not invent it.
    Lost,
}

/// Receives session events.
///
/// The same shape as [`LaunchObserver`], for the same reason: the crate
/// reports, and the host decides how to surface it. Called from the watching
/// thread, so implementations must be cheap and must not block.
///
/// [`LaunchObserver`]: crate::progress::LaunchObserver
pub trait SessionObserver {
    fn on_event(&self, event: SessionEvent);
}

/// Any `Fn(SessionEvent)` is an observer.
///
/// The reason a caller can pass a closure instead of declaring a type for one
/// method. Implementing the trait directly is still there for a receiver that
/// has state worth naming.
impl<F: Fn(SessionEvent)> SessionObserver for F {
    fn on_event(&self, event: SessionEvent) {
        self(event)
    }
}

/// Follow one session until it ends, on a background thread.
///
/// Returns immediately. [`SessionEvent`]s arrive on the observer in the order
/// its docs give, and the thread exits after the terminal one.
///
/// The session record is the Riot Client's own bookkeeping, so it is what
/// answers "why did it stop?" once it is over. It does **not** answer "is the
/// game up" - a client sitting on its own home screen reports phase `None` with
/// the process very much alive - so the process table answers that one, and the
/// watcher reports it as [`SessionEvent::GameRunning`].
///
/// The process table has a second job here. The client can exit while the game
/// keeps running, and the session record goes with the client, so a lookup that
/// answers nothing is not an ending on its own: the watcher keeps watching while
/// `game_process` is alive and reports [`SessionEvent::Lost`] only when both are
/// gone.
///
/// Polling slows from 2 s to 5 s once the game process is up - the impatient
/// part is the wait for it to appear, and after that a session runs for hours
/// with nothing waiting on the answer.
///
/// The returned [`SessionWatch`] calls the whole thing off. Ignoring it leaves
/// the watcher running for the length of the session, which is the
/// fire-and-forget case.
///
/// [`Launcher::watch_session`] is the usual way in, because a launcher already
/// knows the process name. This exists for the caller that holds a session id
/// and no launcher.
///
/// ```no_run
/// use ritoclient::session::{SessionEvent, watch_session};
///
/// let watch = watch_session("irnZWC1kOMt", "LeagueClient.exe", |event| match event {
///     SessionEvent::Ended { exit_code, reason } => println!("over: {reason} ({exit_code})"),
///     event => println!("{event:?}"),
/// });
/// // ... the host is shutting down and no longer cares:
/// watch.stop();
/// ```
///
/// [`Launcher::watch_session`]: crate::launch::Launcher::watch_session
pub fn watch_session(
    session_id: impl Into<String>,
    game_process: impl Into<String>,
    observer: impl SessionObserver + Send + Sync + 'static,
) -> SessionWatch {
    let session_id = session_id.into();
    let game_process = game_process.into();
    let stopped = Arc::new(AtomicBool::new(false));
    let watch = SessionWatch {
        stopped: Arc::clone(&stopped),
    };

    std::thread::spawn(move || {
        // One connection for the whole session, like the hider's - but with
        // retries, which the hider does not need: here a request lost to the
        // port change that waking the client causes would read as a missing
        // session, and a missing session is evidence this watcher acts on.
        let client = Client::builder()
            .retry(RetryPolicy::attempts(3))
            .build()
            .ok();
        if client.is_none() {
            tracing::debug!("Could not build a client; the session can only be watched for loss");
        }

        let mut tracker = SessionTracker::new();
        while !stopped.load(Ordering::Relaxed) {
            let session = client
                .as_ref()
                .and_then(|client| client.product_session().external_session(&session_id));
            let game_alive = crate::processes::is_running(&game_process);
            if tracker.step(session.as_ref(), game_alive, &observer) {
                break;
            }
            std::thread::sleep(tracker.poll_interval());
        }

        // Whether it finished or was called off, it is over - so the watch
        // reports what a caller would otherwise have to guess.
        stopped.store(true, Ordering::Relaxed);
    });

    watch
}

/// The watcher's judgement over one poll, kept apart from the thread that
/// drives it so tests can run it over hand-built sessions.
struct SessionTracker {
    /// The phase last seen. `None` until the first successful lookup.
    phase: Option<SessionPhase>,
    /// Whether the game process was up at the last poll. `None` until the
    /// session opens, so the first reading rides on `Opened` rather than
    /// arriving as a change from nothing.
    game_running: Option<bool>,
}

impl SessionTracker {
    fn new() -> Self {
        Self {
            phase: None,
            game_running: None,
        }
    }

    /// Digest one poll into events, reporting whether the watch is over.
    ///
    /// `game_alive` is the process table's word, and it carries two different
    /// jobs: it is what the host actually means by "the game is up", and it is
    /// what stops an unanswered lookup from reading as an ending.
    fn step(
        &mut self,
        session: Option<&Session>,
        game_alive: bool,
        observer: &dyn SessionObserver,
    ) -> bool {
        let Some(session) = session else {
            // The record goes with the Riot Client, the process does not - so
            // an unanswered lookup ends the watch only once the game is gone
            // too.
            if game_alive {
                return false;
            }
            observer.on_event(SessionEvent::Lost);
            return true;
        };

        let phase = session.phase();
        let opening = self.phase.is_none();
        match self.phase.replace(phase.clone()) {
            None => observer.on_event(SessionEvent::Opened {
                phase,
                version: session.version.clone(),
                game_running: game_alive,
            }),
            Some(previous) if previous != phase => {
                observer.on_event(SessionEvent::PhaseChanged {
                    from: previous,
                    to: phase,
                });
            }
            Some(_) => {}
        }

        // Suppressed on the poll that opened the session, because `Opened`
        // carried this same reading - a host must not be told twice.
        let previously = self.game_running.replace(game_alive);
        if !opening && previously != Some(game_alive) {
            observer.on_event(SessionEvent::GameRunning {
                running: game_alive,
            });
        }

        if session.has_ended() {
            observer.on_event(SessionEvent::Ended {
                exit_code: session.exit_code,
                reason: session.exit_reason(),
            });
            return true;
        }
        false
    }

    /// Slows once the game is up.
    ///
    /// The impatient part of a watch is the wait for the process to appear,
    /// because a host is holding a progress bar on it. Once it is there the
    /// session runs for hours and nothing is waiting on the answer, so the
    /// polling backs off - keyed on the process rather than on the phase, which
    /// can sit at `None` for an entire session.
    fn poll_interval(&self) -> Duration {
        match self.game_running {
            Some(true) => SESSION_POLL_INTERVAL,
            _ => POLL_INTERVAL,
        }
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

    /// The watcher's judgement, driven over hand-built sessions - the thread
    /// around it only supplies the polling.
    mod watching {
        use super::*;

        use std::sync::Mutex;

        /// Captures the events a watch reported, in order.
        #[derive(Default)]
        struct Recorder(Mutex<Vec<SessionEvent>>);

        impl Recorder {
            fn events(&self) -> Vec<SessionEvent> {
                self.0.lock().unwrap().clone()
            }
        }

        impl SessionObserver for Recorder {
            fn on_event(&self, event: SessionEvent) {
                self.0.lock().unwrap().push(event);
            }
        }

        fn session(phase: &str, reason: &str) -> Session {
            Session {
                phase: phase.to_string(),
                exit_reason: reason.to_string(),
                version: "1.0.0".to_string(),
                ..Session::default()
            }
        }

        /// The common life of a launch: the client mints a session, the process
        /// turns up a few seconds later, the phase moves once a match starts,
        /// then hours of nothing - and the polling slows as soon as the game is
        /// there, not when a match is.
        #[test]
        fn a_session_is_opened_then_followed_into_gameplay() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();

            let pending = session("Pending", "StillRunning");
            assert!(!tracker.step(Some(&pending), false, &recorder));
            assert_eq!(tracker.poll_interval(), POLL_INTERVAL);

            // The process appears while the phase has not moved at all.
            assert!(!tracker.step(Some(&pending), true, &recorder));
            assert_eq!(tracker.poll_interval(), SESSION_POLL_INTERVAL);

            let playing = session("Gameplay", "StillRunning");
            assert!(!tracker.step(Some(&playing), true, &recorder));

            // Steady state is silent - a host must not get an event per poll.
            assert!(!tracker.step(Some(&playing), true, &recorder));

            assert_eq!(
                recorder.events(),
                vec![
                    SessionEvent::Opened {
                        phase: SessionPhase::Pending,
                        version: "1.0.0".to_string(),
                        game_running: false,
                    },
                    SessionEvent::GameRunning { running: true },
                    SessionEvent::PhaseChanged {
                        from: SessionPhase::Pending,
                        to: SessionPhase::Gameplay,
                    },
                ]
            );
        }

        /// The reading `Opened` already carried must not arrive twice. A host
        /// that acts on `GameRunning` would otherwise announce the game twice
        /// for every session it recovers.
        #[test]
        fn a_session_open_under_a_live_game_reports_it_once() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();

            let live = session("None", "StillRunning");
            assert!(!tracker.step(Some(&live), true, &recorder));
            assert!(!tracker.step(Some(&live), true, &recorder));

            assert_eq!(
                recorder.events(),
                vec![SessionEvent::Opened {
                    phase: SessionPhase::Nothing,
                    version: "1.0.0".to_string(),
                    game_running: true,
                }]
            );
        }

        /// The phase is not the game. Recorded from client 137: a player sitting
        /// in the client reports `None` with the process very much alive, so a
        /// host keying "is it up" off the phase waits forever.
        #[test]
        fn the_game_is_reported_running_while_the_phase_says_nothing() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();

            let idle = session("None", "StillRunning");
            tracker.step(Some(&idle), false, &recorder);
            tracker.step(Some(&idle), true, &recorder);

            assert_eq!(
                recorder.events().last(),
                Some(&SessionEvent::GameRunning { running: true })
            );
        }

        /// A game closed between matches, with the session still open. The host
        /// hears the process go before it hears the session end, and both.
        #[test]
        fn the_game_going_away_is_reported_on_its_own() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();

            let live = session("None", "StillRunning");
            tracker.step(Some(&live), true, &recorder);
            assert!(!tracker.step(Some(&live), false, &recorder));

            assert_eq!(
                recorder.events().last(),
                Some(&SessionEvent::GameRunning { running: false })
            );
            assert_eq!(tracker.poll_interval(), POLL_INTERVAL);
        }

        /// `Ended` reports the client's own numbers, not a reading of ours.
        #[test]
        fn an_ended_session_reports_the_clients_own_numbers() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();
            tracker.step(Some(&session("Gameplay", "StillRunning")), true, &recorder);

            let mut over = session("None", "Exit");
            over.exit_code = 1;
            assert!(tracker.step(Some(&over), false, &recorder));

            assert_eq!(
                recorder.events().last(),
                Some(&SessionEvent::Ended {
                    exit_code: 1,
                    reason: TerminationReason::Exit,
                })
            );
        }

        /// The case survey section 1.3 records: the Riot Client exits and takes
        /// the session record with it while the game keeps running. Not an
        /// ending on its own - `Lost` needs the process gone too.
        #[test]
        fn a_missing_lookup_is_not_an_ending_while_the_game_lives() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();
            tracker.step(Some(&session("Gameplay", "StillRunning")), true, &recorder);

            assert!(!tracker.step(None, true, &recorder));
            assert!(tracker.step(None, false, &recorder));

            assert_eq!(
                recorder.events().last(),
                Some(&SessionEvent::Lost),
                "no reason arrived, so the watcher must not invent an Ended"
            );
        }

        /// A watch started late still tells the whole story: the first look at
        /// an already-finished session opens it and ends it in one step.
        #[test]
        fn a_session_that_already_ended_opens_and_ends_in_one_step() {
            let mut tracker = SessionTracker::new();
            let recorder = Recorder::default();

            assert!(tracker.step(Some(&session("None", "Exit")), false, &recorder));

            assert_eq!(
                recorder.events(),
                vec![
                    SessionEvent::Opened {
                        phase: SessionPhase::Nothing,
                        version: "1.0.0".to_string(),
                        game_running: false,
                    },
                    SessionEvent::Ended {
                        exit_code: 0,
                        reason: TerminationReason::Exit,
                    },
                ]
            );
        }

        /// A closure is the common observer, and the blanket impl is what lets
        /// one be passed without a type in between.
        #[test]
        fn a_closure_can_observe_events() {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let recorder = Arc::clone(&seen);
            let observer = move |event: SessionEvent| recorder.lock().unwrap().push(event);

            let mut tracker = SessionTracker::new();
            assert!(tracker.step(None, false, &observer));

            assert_eq!(seen.lock().unwrap().as_slice(), &[SessionEvent::Lost]);
        }
    }
}
