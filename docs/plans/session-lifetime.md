# Plan: following a session to its end

What a host needs after `launch()` returns, and what the crate does not give it yet.

Written 2026-08-19 against `68ab5c1`. The consumer that found these is ltk-manager, which is
migrating from the pre-split revision. Its half of the work is
`X:\dev\ltk-manager\docs\plans\ritoclient-launcher.md`.

## The one sentence version

`launch()` answers with a session id, and there the crate stops. Every host that wants to know the
game started, or why it stopped, writes the same poll loop over `external_session` - which is
orchestration, and orchestration lives here.

## What the crate has

The pieces are all present. Nothing joins them.

| Piece                                   | Where                | State                                     |
| --------------------------------------- | -------------------- | ----------------------------------------- |
| `LaunchOutcome.session_id`              | `launch.rs:120`      | Populated on every route that has one     |
| `ProductSessionHandler::external_session` | api namespace      | Reads one session by id                   |
| `SessionExt::phase` / `has_ended`       | `models_ext.rs:205`  | Types both wire strings                   |
| `SessionWatch`                          | `session.rs:41`      | A stop handle, used only by the window hider |
| `open_session_id`                       | `launch.rs:553`      | Finds the live session for a target. **Private** |

A host holding a session id can reach none of that through `Launcher`. The crate-level example
builds a second `Client` by hand to read one field, which is the shape of a missing method.

## 1. A session observer, and the watcher that drives it - **done**

`session` is documented as the home for orchestration that outlives a request. It holds one such
watcher today, and that watcher hides a window. Following the session itself belongs beside it.

```rust
/// What a watched session did.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SessionEvent {
    /// The client opened the session, at the phase it opened in.
    Opened { phase: SessionPhase, version: String },
    /// The phase moved.
    PhaseChanged { from: SessionPhase, to: SessionPhase },
    /// The session ended, and the client said why.
    Ended { exit_code: i64, reason: TerminationReason },
    /// The client stopped answering for this session and the game process is
    /// gone. Separate from `Ended` because no reason arrived - a host that
    /// reports one must not invent it.
    Lost,
}

pub trait SessionObserver {
    fn on_event(&self, event: SessionEvent);
}

impl<F: Fn(SessionEvent)> SessionObserver for F { /* … */ }
```

On the launcher:

```rust
impl Launcher {
    /// Follow a session until it ends, on a background thread.
    pub fn watch_session(
        &self,
        session_id: impl Into<String>,
        observer: impl SessionObserver + Send + Sync + 'static,
    ) -> SessionWatch;
}
```

Design notes, each against a rule this workspace already settled:

- **`SessionObserver` mirrors `LaunchObserver`.** Same shape, same blanket impl for a closure, same
  reason: the crate reports, and the host decides how to surface it.
- **`SessionWatch` is the handle.** It exists, it is `Clone`, and it does not cancel on drop. Reuse
  it rather than adding a second stop type.
- **Two poll intervals**, as the window hider already uses: 2 s while the session reports `Pending`,
  5 s once it reports `Gameplay`. A session runs for hours, and nothing is waiting on the answer.
- **The process scan stays.** Survey section 1.3 records the case where the Riot Client exits and
  the game keeps running. The session record goes with the client, the process does not. So a
  lookup that answers `None` is not an ending on its own: the watcher asks the process table, keeps
  watching while the game is alive, and reports `Lost` only when both are gone.
- **`Ended` reports the client's own numbers.** `exit_code` is meaningful only once `has_ended()` is
  true, which is exactly when this event fires.

As built: the watcher's judgement is a step function (`SessionTracker`), driven by the thread and
tested over hand-built sessions. `watch_session` is also a free function in `session`, beside
`hide_for_play_session` and for the same caller: one that holds an id and no launcher. Its client
retries three times, because a request lost to the port change that waking causes would read as a
missing session, and a missing session is evidence this watcher acts on.

## 2. Reaching the session from the launcher - **done**

Two read-only methods, answering `Option` as every read-only call here does:

```rust
impl Launcher {
    /// The session the client currently has open on this target.
    pub fn session(&self) -> Option<Session>;

    /// The id of that session.
    pub fn session_id(&self) -> Option<String>;
}
```

`session_id` is `open_session_id` made public against the launcher's own target. It already skips
ended sessions, which is the behaviour both methods need.

This is what lets a host recover. A manager restarted while a game runs holds no outcome and no id.
It asks the launcher, gets the session back, and resumes watching. Without it the only answer is a
process name, which is the guess the session id exists to replace.

As built: `open_session_id` became `open_session`, answering the id and the record together, and
both methods read from it. It lost its Windows gate - nothing in it is platform-specific, and on a
machine with no client it answers `None` like any other read.

## 3. Drop the `ts` feature - **done**

`docs/consumers.md` recommends this and the reasoning has not changed. `ts_rs` exports through a
generated `#[test]`, and Cargo never runs a dependency's tests, so the feature produces nothing for
the consumer it was added for. That consumer hand-maintains six binding files today and gets no
warning when one goes stale.

Dropping it removes `ts-rs` from the facade's dependencies and the `exclude = ["bindings/"]` line
from its manifest. It is cheap now and breaking after the first publish.

The downstream half is for ltk-manager to own the shapes that cross its IPC boundary, which its own
plan covers.

## 4. `RiotClientInstalls.patchlines` - **done**

Finding 8 of `launch-flow-136.md`, still open. `RiotClientInstalls` models `associated_client`,
`rc_default` and `rc_live`. The document records `patchlines` as authoritative for the patchline to
client mapping, retried with a trailing `Win` stripped.

It matters when the caller passes no `product_root`. ltk-manager's league path is a user setting and
can be unset, and the fallback is then `rc_default` or `rc_live` - which on a machine with several
Riot products is a coin flip about which client owns the install.

**Confirm the field's shape against a live `RiotClientInstalls.json` before writing the struct.** The
document names the key and the `Win` retry, and this plan does not assume more than that.

Confirmed 2026-08-19 against a live file: a string map, `"KeystoneFoundationLiveWin"` to the client
exe path. Modelled as a public field plus `patchline_client`, which retries with a trailing `Win`
stripped from the query - the direction `FindRiotClientForPatchline` goes. `candidates()` is
unchanged on purpose: it has no patchline input, and the keys name the Riot Client's own patchline,
which the resolver cannot derive from a game's install root. Feeding it one is a caller's decision,
still open.

## 5. Wanted, not blocking

- **A cancellable launch - decided and built.** `wait_for_launcher` ran to a 120 s `BOOT_TIMEOUT`
  with no way in. A host that offers a Cancel button could not honour it, and a host that
  serialises launches had its button dead for two minutes when a client never came up. As built: a
  caller-made `StopFlag` handed to `Launcher::launch_with_stop`, checked between the steps of the
  wait. Per call rather than on `Launcher`, because a `Launcher` is shared - stopping one launch
  must not stop every clone's. A stop answers `LauncherError::Stopped` and reports
  `LaunchStage::Stopped`, not `Error`, so a listener does not put an error dialog behind its own
  Cancel button. Stopping abandons the wait and never the launch - a POST the client accepted
  keeps going at the far end.
- **Hide on the session rather than the process table.** `hide_for_play_session` waits for the game
  process with a 300 s `GAME_WAIT_TIMEOUT`, then stops. A cold start that patches several gigabytes
  passes that, and the hide silently does nothing. Once section 1 exists the hider can wait for
  `SessionPhase::Gameplay` instead, and the timeout stops being a guess about download speed.

  Probed 2026-08-19 for a client API that would make the restructure unnecessary (booted
  137.0.3.4826, all 1275 functions swept - the findings live in `docs/riot-client-local-api.md`,
  "The Riot Client's own window"). No "stay hidden" lever exists, so the session wait above stays
  the fix for the *first* hide. The probe did find a better tool for the *second*: the game-exit
  un-hide is a pending `UxCommand { showUxIfHidden }`, and
  `DELETE /riot-client-lifecycle/v1/ux-command` removes it before the UX processes it. If that
  holds against a real game exit, the 10 s re-hide loop becomes one delete.

## 6. Order

Section 1 and section 2 land together - the watcher needs the session id lookup, and the lookup is
half a feature without the watcher. Section 3 is independent and cheapest before a publish. Section
4 is independent and small. Section 5's first item is built. Its second waits on one live test:
whether deleting the pending ux-command beats the UX to it.

## 7. Testing

Unit tests cover what does not need a client:

- `SessionEvent` transitions, by driving the watcher's step function over hand-built `Session`
  values. The interesting cases are `Pending` to `Gameplay`, a lookup that answers `None` while the
  process lives, and the same lookup once it does not.
- `SessionWatch` stop and clone behaviour, which the existing tests already cover for the hider.

`examples/launch.rs` grows a watch and prints the events, which is the only place the real thing
runs.
