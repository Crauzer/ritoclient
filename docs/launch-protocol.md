# Launching, and the routes that do not work

What `launch()` is actually doing, and the approaches that were tried and rejected - recorded so
nobody re-discovers them and assumes they were overlooked.

Derived from Riot Client `135.0.3.4731`, League `16.14.794.5912`, and revisited against
`136.0.3.4787` - the build where the wake call stopped being inert.

## The rule that shapes everything

**A game's argv carries an `rso_auth.authorization-key` blob that only an authenticated Riot Client
can mint.** Any design that starts `LeagueClient.exe` directly is therefore wrong, regardless of how
tempting the process tree makes it look - the process starts and then fails to authenticate.

This is the one item in the crate's "does not do" list that is a technical fact rather than a
posture. Everything else about launching follows from it: the Riot Client must be the one to start
the game, so the crate's job is to get a request to it.

## Three states, three routes

`launch()` is orchestration rather than a namespace because the route depends on what is running:

| Client state    | What happens                                                        |
| --------------- | ------------------------------------------------------------------- |
| Running         | `POST /product-launcher/v1/products/{productId}/patchlines/{patchlineId}` |
| Tray-idle       | Wake it first, then the above                                        |
| Not running     | Cold-start `RiotClientServices.exe` bare, wait, then the above       |

The routes differ only in how the client gets to a state where it can answer. **The POST is the only
thing in the process that starts a game**, and that is a property worth stating as a rule rather than
an implementation detail - see "The wake carries no arguments" below.

The two-step for a tray-idle client is not defensiveness. **A tray-idle client collapses its whole
API to `/riotclientapp/v1/new-args`** - 1,162 bytes of `/help` against 293,077 booted, with
`PostRiotclientappV1NewArgs` the only surviving function. The `product-launcher` plugin is not
registered yet, so the launch route genuinely 404s until it finishes booting. That 404 means "wait",
which is the clearest example of why status classification belongs to the caller.

## Asking readiness instead of inferring it

`GET /product-launcher/v1/is-launch-request-pending` is the readiness probe: a plain GET that mutates
nothing, on a route that exists only once the plugin is mounted. Its answer is read three ways, off
the `errorCode` split the client gives on its 404s:

| answer | means | what the wait does |
| ------ | ----- | ------------------ |
| 200 `false` | mounted, nothing in flight | send the launch POST |
| 200 `true` | a launch is already in flight | wait; the POST is not idempotent |
| 404 `RESOURCE_NOT_FOUND` | the plugin is not registered - tray-idle | wake, then keep polling |
| 404 `RPC_ERROR` | registered, handler not up yet | keep polling |
| no answer at all | listener restarting, or no client | keep polling |

The point of asking is what it does to the *launch* POST. Fired only when the launcher has just said
it is serving, its status can mean one of two things - launched, or refused - so a 404 from it is now
about the product or the patchline rather than the route. A product id nobody has fails in a second
instead of being retried until the boot budget runs out.

Waking is itself a source of transport failures: it **restarts the remoting listener on a new port
under the same pid** - 64699 to 60865 to 63057 under one pid across a single session. This is why
`Client` re-reads the lockfile on every attempt rather than storing a port, and why retrying
transport errors is worth more than it looks - a retry picks up the new port.

## The wake carries no arguments

`new-args` is the wake primitive because in the tray-idle state it is the only route that exists. It
is **not** a launch route, and the reason is not that it cannot launch - on 136 it can:

```
POST /riotclientapp/v1/new-args
["--launch-product=league_of_legends","--launch-patchline=live"]
-> 204, and LeagueClient.exe came up ~8.4 s later
```

On 135 the identical call answered 204 and launched nothing. Both measurements are real: the client
that launched is cohorted into the lifecycle rewrite (`RC_15.new_lifecycle: "globalEnable"`). So its
204 means "arguments accepted" and carries no information about what happens next, which is enough on
its own to disqualify it.

What it launches is also the wrong launch. `App_OnNewArgs` publishes the raw array before filtering,
the lifecycle launch-args object re-parses every switch in it, and the launch runs through the whole
startup middleware chain - including `DirectLaunchMiddleware`, which on an install inside the August
2026 rollout shows the window and returns *without launching*. It hands back no session id, and it
can race the `product-launcher` POST the wait loop is about to send.

**So we wake with `[]`.** Same 204, same restarted listener, nothing for any build or cohort to act
on. The cold start is bare for the same reason: `RiotClientServices.exe` with no `--launch-product`
boots to the window a user who opened it themselves would see, and our POST is then literally the
Play button. `POST /product-launcher/...` is not gated and does not touch that chain.

`launch()` returns when the request is *delivered*, not when the game is up. The client may still
patch or wait for a login.

## One route, three verbs

`/product-launcher/v1/products/{productId}/patchlines/{patchlineId}` is a resource, not a call:

| verb | does | answers |
| ---- | ---- | ------- |
| `POST` | launch it | the session id, as a bare JSON string |
| `DELETE` | close it | 204; the game is gone in under six seconds |
| `PUT` | adopt a running one | a session id, same shape as the launch |

`PUT` is the client's own recovery path - *"Recover a session for a product that is already running,
but Riot Client Services doesn't know about since it just started up."* `launch()` uses it without
being asked: when the game is already up it looks for a session first, and only hands over the pid
when the client has none. That is what turns "League is already running" from a dead end into an
outcome with a session id attached, and it is why `LaunchRoute` distinguishes `ADOPTED` from
`ALREADY_RUNNING`.

The pid goes in the body as a bare JSON number, which is this client's convention for a single
non-path argument. **Unconfirmed on the wire** - the other reading is a query parameter, which
nothing in the workspace models. `DELETE`'s optional `shouldTerminateProcess` has the same question
over it and is not modelled at all; the plain close is what was measured.

## Rejected approaches

**`riotclient://product/launch/v1/league_of_legends/live`** - a URI scheme registered under
`HKCR\riotclient`. It reaches the same handler as the HTTP route with strictly less error
visibility: it goes through `ShellExecute` and gives no success signal at all. Not used.

**Spawning the game directly** - see the rule above.

**`--launch-background-mode`** on the cold-start path - plausible and deferred, because it is a
behaviour change rather than a refactor. Note also that the idle surface it produces is the 1,162-byte
one above, so a client parked there cannot be asked to launch until it is woken again.

**`--allow-direct-launch`** - it exempts a lifecycle launch from the direct-launch gate. Since nothing
we send goes through the lifecycle path, there is nothing for it to exempt.

## Detecting that the game is up

Two answers, and both are load-bearing.

**`/product-session/v1/external-sessions`** is Riot's own bookkeeping rather than a toolhelp
snapshot. It carries `phase` and, once a session ends, `exitCode`/`exitReason` - so a game that
started and died immediately gives a reason instead of silence - and reports patchline and version
for free. A launch returns the session id that keys it, and `LaunchOutcome.session_id` now carries
one on every route that has one, including `ALREADY_RUNNING`: a game we did not start still has a
session, and the client will name it.

Read the two enum fields through `SessionExt` rather than by hand. Both have a value that reads like
its opposite at a glance - a session at `Pending` is not playing, and one reporting `StillRunning`
has not ended.

**The process scan** (`processes::is_running`, with the executable supplied by the caller so the
library still names no products) stays, and not only as a fallback. Survey section 1.3 flags the case
where the Riot Client exits while the game keeps running: the session record goes with the client,
the process does not. It is also the only answer available before a client exists at all, which is
why the launch flow still opens with it.
