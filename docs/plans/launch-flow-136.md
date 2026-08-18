# Plan: the launch flow after Riot Client 136

Rebasing `ritoclient::launch` on the 2026-08-15 revision of
`D:\lol\league_structs\docs\reversing\RiotClient_ProductLaunch.md` (Riot Client 136.0.3.4787).

That revision rewrites its sections 7, 9, 9.1 and 12 against the build where Riot switched on the
*direct launch* gate. Two of the corrections invalidate assumptions our launcher is built on, and one
of those is a live bug on the path a mod launcher hits most.

Section numbers below refer to that document.

## The one sentence version

`new-args` is no longer inert, so our wake call is now a second, unguarded launch on the gated
lifecycle path - and our cold start returns "launched" without ever checking that anything launched.
Both are fixed by driving `POST /product-launcher/...` for **every** launch and using `new-args`
only as a bare wake, which also makes the orchestration smaller than it is today.

## What changed

| # | Finding | Section | Our code today | Verdict |
| - | ------- | ------- | -------------- | ------- |
| 1 | `POST /riotclientapp/v1/new-args` with the launch pair **does launch** on 136, walking the full middleware chain, ~8.4 s | 9.1.2 | `launch::wake()` sends `--launch-product` + `--launch-patchline` as its wake | **Bug.** Our wake is now a launch we do not track, on the gated path, with no session id |
| 2 | Cold start must poll for the client and then POST; the spawned RC's own auto-launch is subject to the direct-launch gate | 9.1 step 5, 12.1 | `launch_inner` spawns and returns `Ok(ColdStart)` immediately | **Bug.** On a gated install the RC window opens, nothing launches, and we report success |
| 3 | `GET /product-launcher/v1/is-launch-request-pending` is the cheapest "is the plugin registered" probe: 200 ready, 404 tray-idle, connection refused cold | 9.1 step 3, 9.1.1 | We fire the launch POST first and classify its 404 / 5xx / transport error / missing lockfile into one `NotReady` | **Restructure.** Readiness gets its own question, so the launch POST stops doubling as a probe |
| 4 | `product-launcher` is **not** subject to the direct-launch gate; the lifecycle path is. Proven live and statically | 9, 12.1 | We reach product-launcher eventually, but two paths can start the game | **Confirms the destination, forbids the detour** |
| 5 | `/product-session/v1/external-sessions` is keyed by the session id the launch returns and carries `phase`, `exitCode`, `exitReason` | 9.1 step 6 | Deferred in `api-surface-codegen.md`; detection is a caller-supplied process name | **Promote.** It is now part of the flow, not an improvement to it |
| 6 | `DELETE /product-launcher/v1/products/{p}/patchlines/{pl}` closes the product, 204, gone in <6 s | 9.1.2 | Not modelled | New capability |
| 7 | `PUT` the same path with a pid adopts a product we did not start | 9.1.1, 11.3 | Not modelled; "already running" is terminal | New capability |
| 8 | `RiotClientInstalls.json` `patchlines` is authoritative for patchline to RC, retried with a trailing `Win` stripped | 2 | `RiotClientInstalls` reads `associated_client`, `rc_default`, `rc_live` only | Minor resolver gap |
| 9 | `direct-launch-opted-in` is a durable install-setting, POST to write, `null` to clear, no delete verb | 12.3 | Not modelled | Optional user-facing toggle, not needed to launch |
| 10 | `--initial-route` is the only argument that survives `customLaunchArgs` filtering, and its value is unfiltered | 4, 11.2 | Body is hardcoded `{}` | Optional |
| 11 | `PUT /rnet-product-registry/v4/session-patch-lock/...` `isExclusive` is the candidate "do not patch over my staged files" lock, still untested | 9.1.1, open items | Not modelled | Flag only, and lower priority than it reads - LTK writes nothing into the install ahead of a launch |

### What finding 1 costs us

Not mod timing. LTK applies mods with an injector DLL that runs when the *game* client starts, well
after this flow has finished, so nothing here can outrun the staging. What the wake costs is control:
`App_OnNewArgs` publishes the raw array, the lifecycle launch-args object re-parses all fourteen of
its switches, and the chain runs to `WaitForLaunch`. That launch is on the gated path, hands back no
session id, and can race the `product-launcher` POST our own wait loop is about to send.

### Why finding 2 is the headline

`launch_inner`'s cold-start arm is the common case for a mod launcher: no Riot Client running. It
spawns `RiotClientServices.exe --launch-product=... --launch-patchline=...` and returns
`LaunchOutcome { route: ColdStart, session_id: None }` without a single further check. On an install
inside the August 2026 rollout bucket, `DirectLaunchMiddleware::run` logs *"Showing UX for player as
Direct Launch is Not enabled"* and returns - the window appears, the game does not. We have already
told the caller it worked.

## What the document confirms - leave these alone

Worth stating so the rewrite does not churn them:

- **Re-read the lockfile per attempt.** Section 9.2 measures 64699 to 60865 to 63057 under one pid.
  `Client::attempt` already does this and `Lockfile`'s doc comment already records why.
- **Never cold-start over a live client.** Section 5's footgun (the loser's handoff has a 5 s budget,
  then it *terminates* the running client) is why `launch_inner` checks `live_lockfile()` first.
- **Eligibility is an entitlement check.** Section 9.1.2 re-confirms `true` for `pbe` with no PBE
  install. Our `is_eligible` doc comment already says exactly this, and says to test
  `install_full_path` instead.
- **Hide, do not switch to background mode.** Section 9.3 measures the idle surface at 1,162 bytes
  with only `PostRiotclientappV1NewArgs` surviving - the third reason our `lifecycle` module docs
  already give for rejecting it.
- **Never spawn the game.** Section 6d's `rso_auth.authorization-key` is the unfakeable handoff.
- **A status is data.** Section 9.1.1's table is a list of statuses that mean different things per
  route, which is the rule the crate is built on.

## The target flow

```
launch(product_root, target, game_process, observer)

0  resolve RiotClientServices.exe            installs.rs, unchanged
1  already up?                               process scan, then external-sessions once a client exists
2  live_lockfile()?  no  -> spawn RCS bare, fall through
3  wait until ready:                         GET /product-launcher/v1/is-launch-request-pending
       Serving        -> ready
       Absent (404)   -> tray-idle: POST new-args []   then keep polling
       NoClient       -> keep polling (listener restarting), or cold start if we never spawned
       pending = true -> a launch is already in flight; skip to 5
4  launch once:                              POST /product-launcher/v1/products/{p}/patchlines/{pl}
       200            -> session id
       refusal        -> retry inside REFUSAL_GRACE, then report it as itself
5  confirm:                                  GET /product-session/v1/external-sessions/{sessionId}
```

Three properties this has that the current shape does not:

1. **One launch verb.** `new-args` never carries a launch pair, the cold start never carries one, so
   `POST /product-launcher/...` is the only thing in the process that can start a game.
2. **Readiness is asked, not inferred.** The launch POST is fired once, when the plugin is known to
   be there, and its status therefore means exactly one of two things: launched, or refused.
   `LaunchAttempt::NotReady`'s four-shapes-one-meaning comment stops being needed.
3. **The gate is structurally unreachable.** Nothing we send goes through RiotClientLifecycle, so
   `direct_launch_opt_in` cannot decide whether our launch happens. Section 9's closing line, made
   into a property of the code.

### Cold start: spawn bare

The document's step 5 suggests spawning with `--launch-product`, `--launch-patchline` and
`--allow-direct-launch`, then polling. Recommend spawning with **none of them**:

- The RC's own auto-launch would race our POST. There is a real window - the chain reaches
  `WaitForLaunch` only after `PatchStatus`, `PlayerAffinitySync` and the rest, while
  `is-launch-request-pending` stays false until product-launcher is actually called - in which the
  game gets launched twice. Section 9.1.1 already notes the POST is not idempotent.
- It only ever buys a few seconds, and only for users *outside* the rollout bucket - the ones who did
  not need help.

The race is the whole argument. Mod staging is not a consideration either way: LTK injects at game
start, not before the Riot Client's launch.

A bare spawn boots the RC to its normal window, which is the state a user who opened it themselves
would be in, and our POST is then literally the Play button. The window gets hidden by
`session::hide_for_play_session` as it already does. The cost is that a cold start no longer shows the
product splash screen, which is cosmetic and only visible for the couple of seconds before the hide.

`--allow-direct-launch` is then irrelevant to us, exactly as section 9.1's closing note says. It stays
worth knowing about for the optional toggle in P5.

## Phases

Ordered so each phase is independently shippable and the two bugs go first.

### P0 - correct the record (no code) - **done**

Our measured prose is now wrong in three places, and it is prose the project treats as irreplaceable.
`schema/overrides.toml` is the single home for the `ritoclient-api` half, so it is edited there and
the crate is regenerated, never hand-edited.

| File | What is wrong |
| ---- | ------------- |
| `schema/overrides.toml`, `riotclientapp` `module_doc` | "against a fully booted client the documented launch body returns 204 and launches nothing at all" - false on 136 |
| `schema/overrides.toml`, `PostRiotclientappV1NewArgs` `route_doc` / `endpoint_doc` / `method_doc` | The same "204 means queued, not launched" claim, three times |
| `crates/ritoclient/src/launch.rs`, `wake()` | "`new-args` queues argv and nothing more" |
| `docs/launch-protocol.md` | The tray-idle two-step description, and the "Detecting that the game is up" section |
| `schema/overrides.toml`, `product-launcher` `module_doc` | Found while editing, not in the original count: "that one *queues arguments* and answers 204 without launching anything" |

The replacement wording should keep *both* measurements and date them, because the old one was
correct on 135 and the difference is a cohort (`RC_15.new_lifecycle: "globalEnable"`), not a mistake.
The rule that survives both builds is the one to lead with: **204 means "arguments accepted", and what
acts on them is a build- and cohort-dependent question, so never use it to launch.**

### P1 - the two bugs - **done**

Smallest change that removes both, no restructure:

1. `launch::wake()` sends `[]` instead of the launch pair. The empty-argv wake is already proven in
   this repo - `xtask/src/snapshot.rs`'s `ensure_awake` does exactly this and its comment records
   that it "opens the window, launches nothing".
2. `spawn::cold_start` drops both `--launch-product` and `--launch-patchline`, and `launch_inner`'s
   cold-start arm falls into the same wait-then-launch path as the handoff arm instead of returning.
   `LaunchRoute::ColdStart` keeps its meaning (how the client got there) and now carries a session id.

After P1 the cold-start arm and the handoff arm differ only by "did we spawn it", which is what makes
P2 a simplification rather than a rewrite.

As built: `wait_for_launcher` took a `LaunchRoute` parameter so the cold start could reuse it
verbatim, and `spawn::cold_start` lost its `LaunchTarget` argument entirely - it takes only the
executable now. The `Client` is constructed before the client exists, which costs nothing because the
lockfile is read per attempt.

### P2 - readiness gets its own question - **done**

- Add `launch::wait_until_ready(client, deadline, observer)` built on
  `Client::probe` and the three-way `Presence` the core crate already has:
  `Serving` ready, `Absent` tray-idle (wake, then keep polling), `Registered` keep polling,
  `RequestError::NoClient` keep polling.
- `attempt_launch` loses the `NotReady` variant. It returns launched-or-refused, and transport
  failure goes back to `wait_until_ready` rather than being renamed.
- `wait_for_launcher`'s loop keeps three things and sheds the rest: the deadline, the
  `is_launch_request_pending` double-launch guard, and `REFUSAL_GRACE`. The refusal grace stays
  measured and stays needed - section 9.1.1's "RC not signed in" row is the same shape as the
  `eula_not_accepted` we measured, and a client can serve product-launcher before its player state
  has caught up.
- Budgets: `BOOT_TIMEOUT` stays 120 s (a cold start can self-update). Note in the doc comment that
  the ~5 s `ClientConfig` stall of section 7 is a cost of the lifecycle path we no longer pay, and
  that measured post-POST latency is ~3.8 s to `LeagueClient.exe`.

Net effect on `launch.rs`: `LaunchAttempt` collapses to a two-armed result, `wake` becomes
argument-free, `hand_off` and the cold-start arm merge, and the "four shapes, one meaning" doc block
is deleted rather than maintained.

As built, three departures from the sketch above, each because the code wanted it:

- **No separate `wait_until_ready`.** Readiness got its own *question* - a `Readiness` enum and a
  `readiness()` that reads it off one GET - but not its own loop. A second loop would have had to
  share the deadline, the game-process check and the progress emitter with the launch loop, and the
  refusal grace straddles both. One pass asks all three questions in order instead.
- **The probe reads the body too.** `is-launch-request-pending` answers readiness *and* the
  double-launch guard in the same round trip, so `Readiness::Serving` carries `launch_pending`. The
  separate `is_launch_request_pending()` call the old loop made is gone.
- **`attempt_launch` got stricter, not just smaller.** With readiness asked first, a 404 from the
  POST is about the product or patchline rather than the route - so it is refused rather than
  retried, and a bad product id fails in a second instead of spinning out the 120 s budget. The
  `RPC_ERROR` / `RESOURCE_NOT_FOUND` split is what makes that safe: the former still means "the
  launcher went away mid-flight", and is retried.

Also: the poll interval moved to the end of the pass, so a booted client launches on the first pass
with no sleep, and `WaitingForClient` is only reported when there is an actual wait. `is_eligible`
stayed in `hand_off` rather than moving into the loop - a freshly booted client can answer it before
its player state has loaded, so firing it on the cold path would produce a warning that means
nothing.

### P3 - `product-session`, and confirmation - **done**

New generated namespace. `GetProductSessionV1ExternalSessions` and
`...BySessionId` are **already in `schema/help.filtered.json`** (`product-session` is in
`xtask`'s `IN_SCOPE`), so this is `overrides.toml` plus one generator gap, not a re-snapshot.

- `ProductSessionSession` is curated to `product_id`, `patchline_id`, `patchline_full_name`, `phase`,
  `version`, `exit_code`, `exit_reason`. **`launch_configuration` is deliberately omitted**: it
  carries `rso_auth.authorization-key`, and the project's hard rule is that a session dump strips it.
  Leaving the field out of the type makes the rule structurally impossible to break instead of a
  thing to remember. If argv readback is ever wanted, that is a separate decision with a redaction
  policy attached.
- `phase` and `exit_reason` are enums in `/help`; the generator already carries enums as `String`
  under its tolerance policy, so no new machinery.
- `launch()` confirms through the session id it already receives, and the process scan stays as the
  fallback for the case the survey flags: the RC exits while the game keeps running.
- `session::hide_for_play_session` can then watch `phase` instead of walking the process table every
  5 s, and gets `exitReason` for free. Optional within this phase.

As built, one departure and one deliberate omission:

- **The confirmation landed on the `AlreadyRunning` route, not after the launch POST.** Confirming
  straight after a successful POST is worth nothing: `launch()` returns the moment the request is
  delivered, and the session is `Pending` or not yet open for the ~3.8 s until the game appears. The
  place a lookup pays is the branch that never had an id - a game that was already up now comes back
  with the client's own session id via `external-sessions`, so a caller that finds it running gets
  the same handle as a caller that started it. `open_session_id` skips ended sessions, because the
  client keeps finished records around and handing one back as the running game would be worse than
  handing back nothing.
- **`hide_for_play_session` still walks the process table**, and should. Its watch runs for hours,
  and the failure mode of a session-based watch is exactly the case survey section 1.3 flags: the
  Riot Client exits while the game keeps running, taking the session record with it. The process
  scan is the more robust answer for the long watch, not the cheaper one.

The behaviour on the model went to `SessionExt` in the facade, per the invariant that generated types
carry no inherent `impl`. It wraps `phase` and `exit_reason` because both have a value that reads
like its opposite at a glance: `Pending` is not playing, `StillRunning` has not ended.

### P4 - close and adopt - **done**

- `DELETE /product-launcher/v1/products/{p}/patchlines/{pl}` - "close the game" for the manager.
- `PUT` the same path with a pid - turns section 9.1.1's "League already running, we did not start
  it" row from terminal into recoverable. Needs a new `LaunchRoute::Adopted`, and the pid comes from
  the process scan.

As built:

- **Adopting is not a separate entry point.** `launch()` already handled "the game is up"; that
  branch now asks `external-sessions` first and only `PUT`s when the client has no session for the
  target - which is precisely the state the route exists for ("Riot Client Services doesn't know
  about it since it just started up"). So the manager gets adoption without calling anything new,
  and `AlreadyRunning` vs `Adopted` records which happened. Adopting is best-effort: a client that
  will not take the pid leaves the outcome where it was.
- **`Adopted` reports `LaunchStage::AlreadyRunning`, not `Launched`.** Neither route launched
  anything, and a caller watching for `Launched` is watching for a game that started because it
  asked.
- **`LauncherError::LaunchRefused` is now `Refused`.** Closing and adopting are refused the same way
  a launch is, and the variant was only ever about the client answering "no" - `riot_error_code` is
  what says to which request. Wire tag `LAUNCH_REFUSED` becomes `REFUSED`.
- **Three endpoints now share one route**, which turned up a real generator bug: an endpoint that
  deduped onto an existing route kept a route constant derived from its own name, and referenced a
  constant that was never emitted. Shared routes now adopt the declared one. The templates already
  claimed several endpoints could share a route; nothing had tested it.
- **`processes::pid_of`** was added beside `is_running` - adopting needs the pid, and assembling it
  from `list_matching` in the facade would be the facade knowing the process table's shape.

### P5 - optional: the direct-launch opt-in toggle

Section 12.3's `POST /data-store/v1/install-settings/direct-launch-opted-in/{product}.{patchline}`,
verified end to end on 2026-08-15. `data-store` is in `IN_SCOPE` and both verbs are in the snapshot.

This is a **user preference**, not something our launch needs - worth building only if the manager
wants to offer "open League without the Riot Client window appearing". If it does, the off switch is
asymmetric and has to be documented at the API: there is no delete verb, `null` is the correct off,
and the key stays in `RiotClientSettings.yaml`.

## Generator work this needs

Three small gaps in `xtask/src/codegen`, all hit by P3 and P4:

| Gap | Where | Fix |
| --- | ----- | --- |
| ~~`map<string, T>` return types are unhandled~~ **done** | `ModelResolver::map_output` matches `""`, `"vector"` and named types; `"map"` falls through and errors | Added as a shared arm with `vector`, emitting `HashMap<String, T>`; the two emitters add the `std::collections::HashMap` import when an output needs it. Map *fields* still error, but now say so - modelling one would also mean teaching `reachable_from` and the flat header about them, and no field we emit is a map |
| ~~No `body = "none"` override~~ **done** | `body_override` accepts only `empty-object` / `empty-string` | Added. The close ships without `shouldTerminateProcess`, which is the one argument on this namespace never seen on the wire - body or query is unconfirmed, and sending nothing leaves the client on the default that was measured |
| ~~`int32` body arguments are unmapped~~ **done** | `BodyKind::BareArg`'s type match covers `vector<string>`, `string`, `bool` | Added as `i32`. This also fixed a latent bug: the emitter wrote `json_body(self.field)` for every arm, which cannot compile for a value type - `bool` would have hit it too, but no endpoint used it. **Still unconfirmed against a live client** whether the pid rides in the body or the query; the convention says body, and that is what ships |

Everything else is `schema/overrides.toml` plus `cargo xtask ritoclient-codegen`. No re-snapshot is
required for any of P3 through P5.

## Public API impact

The workspace is `0.1.0` and unreleased, so these are free now and expensive later:

- `LaunchRoute` gained `Adopted` (P4). `LaunchStage` gained nothing (P3): a confirming variant would
  have been a distinction the UI should not draw. Both derive `ts_rs::TS` and the manager switches on
  the serialized strings - see `progress.rs`'s wire-name test, which is the right place to keep them
  honest.
- `LaunchOutcome.session_id` is now populated on every route that has one, including
  `ALREADY_RUNNING`. Shape unchanged.
- `LauncherError::LaunchRefused` became `Refused`, so its wire tag went from `LAUNCH_REFUSED` to
  `REFUSED`. The one change here a manager has to act on.
- `launch::close` is new, and `processes::pid_of` alongside `is_running`.

## Risks and open items

- **The gate could not be made to fire on the test install** (section 12.5: the debug override 404s on
  retail, the flag reads `false`). So the gated behaviour is proven statically and by the r/lol
  reports, not observed here. This does not weaken the plan - the fix is to stop depending on the
  gated path at all, which is correct either way - but it does mean P1's cold-start change cannot be
  A/B tested locally.
- **Whether a warm `new-args` launch is gated is open** (section 12.6). Irrelevant after P1, by
  construction.
- **`session-patch-lock` stays untested** and stays unmodelled. The document offers it as the "do not
  patch over my staged files" lock, which matters far less here than it reads: LTK injects at game
  start rather than writing into the League install ahead of a launch. It only becomes a plan item if
  something in the manager does start writing into the install directory.
- **`is-launch-request-pending` as a readiness probe is a 404-vs-200 distinction**, and section 9.3's
  `RPC_ERROR` vs `RESOURCE_NOT_FOUND` split is what makes it reliable. `Client::probe` already models
  it. Watch for `Registered` (plugin present, handler not yet available) being commoner than expected
  on a waking client; the poll loop treats it as "keep waiting", which is right.

## Not doing

- **Passing `--allow-direct-launch` anywhere.** After the bare cold start there is nothing for it to
  affect, and section 9.1.1 already lists "Riot removes it" as a no-impact event for this flow.
- **`riotclient://` URIs.** Unchanged verdict from `launch-protocol.md`: same handler, no success
  signal.
- **`customLaunchArgs`.** `--initial-route` is real and unfiltered, but nothing in the manager wants
  it yet. Recording it in the endpoint docs is enough.
- **Feature-flag debug overrides** (section 12.5). Not implemented on retail.
- **Splitting `prepare()` from `launch()`.** The document's step 4 reads "Apply mods, then: POST",
  which suggests exposing a seam between "client ready" and "launch". LTK does not need one: mods go
  in through an injector DLL at game start, so there is nothing to do between those two points.
  `launch()` stays a single blocking call.
