# Launching, and the routes that do not work

What `launch()` is actually doing, and the approaches that were tried and rejected - recorded so
nobody re-discovers them and assumes they were overlooked.

Derived from Riot Client `135.0.3.4731`, League `16.14.794.5912`.

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
| Not running     | Cold-start `RiotClientServices.exe`, wait, then the above            |

The two-step for a tray-idle client is not defensiveness. **A tray-idle client collapses its whole
API to `/riotclientapp/v1/new-args`** - the `product-launcher` plugin is not registered yet, so the
launch route genuinely 404s until it finishes booting. That 404 means "wait", which is the clearest
example of why status classification belongs to the caller.

Waking is itself a source of transport failures: it **restarts the remoting listener on a new port
under the same pid**. This is why `Client` re-reads the lockfile on every attempt rather than storing
a port, and why retrying transport errors is worth more than it looks - a retry picks up the new
port.

`launch()` returns when the request is *delivered*, not when the game is up. The client may still
patch or wait for a login.

## Rejected approaches

**`riotclient://product/launch/v1/league_of_legends/live`** - a URI scheme registered under
`HKCR\riotclient`. It reaches the same handler as the HTTP route with strictly less error
visibility: it goes through `ShellExecute` and gives no success signal at all. Not used.

**Spawning the game directly** - see the rule above.

**`--launch-background-mode`** on the cold-start path - plausible and deferred, because it is a
behaviour change rather than a refactor.

## Detecting that the game is up

Currently a process-name match, with the executable supplied by the caller (`processes::is_running`)
so the library still names no products.

The better answer is `/product-session/v1/external-sessions`, and it is deferred rather than
rejected - see the plan. It is Riot's own bookkeeping rather than a toolhelp snapshot, carries
`exitCode`/`exitReason` so a game that died immediately gives a reason instead of silence, and
reports patchline and version for free. A launch already returns the session id that keys it.

Keep the process scan as the fallback regardless: survey section 1.3 flags the case where the Riot Client
exits while the game keeps running.
