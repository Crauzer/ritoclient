# Riot Client local API - survey

**This is the crate's source data.** [The codegen plan](./plans/api-surface-codegen.md) generates
from it, and section 6's namespace map is where the in-scope set was chosen. Read it before adding a
namespace.

Reconnaissance done 2026-07-26 against a live client (Riot Client build on patch 16.14, EUW),
and **re-done 2026-07-27** as a complete pass over `/help`'s 1261 `functions` - the step the
first survey skipped, and the reason it missed the launch endpoint.

Everything marked **[verified]** was actually called and returned the payload shown. Items marked
**[unverified - signatures only]** come from `/help?format=Full` and have a known name, argument
list and return type, but were **not** called; several of them mutate state (section 1.9).

> **Provenance.** Written while this crate still lived inside
> [LeagueToolkit/ltk-manager](https://github.com/LeagueToolkit/ltk-manager), so its tiering ("things
> we should actually wire up") ranks by *that* application's needs rather than this crate's.
>
> References below to `league-launch-flow.md`, `pbe-multi-install.md` and `launch-flow-ux.md` point
> at documents that stayed in that repo under `docs/` - they are application contracts, not client
> knowledge. The launch protocol they cover is summarised here in
> [`launch-protocol.md`](./launch-protocol.md).
>
> The measurements are unaffected and kept verbatim; they are the part that cost a live client to
> obtain. Treat the priorities as historical and the payloads, statuses and spellings as current.

---

## 0. How to talk to it

Same transport as the launch flow - reuse the crate-local HTTP client from
`crates/ritoclient-api/src/http.rs`:

- Lockfile: `%LOCALAPPDATA%\Riot Games\Riot Client\Config\lockfile`
  → `name:pid:port:password:protocol`
- `https://127.0.0.1:{port}`, `Authorization: Basic base64("riot:" + password)`
- Self-signed cert → `danger_accept_invalid_certs(true)`, scoped to this module only.

**Discovering the surface.** The client documents itself:

| Path                                    | What                                                               |
| --------------------------------------- | ------------------------------------------------------------------ |
| `GET /swagger/v3/openapi.json`          | 782 paths, full schemas - **[verified]**                           |
| `GET /swagger/v2/swagger.json`          | same, Swagger 2.0                                                  |
| `GET /help`                             | `events` (61), `functions` (1261), `types` (3966) - **[verified]** |
| `GET /help?format=Full`                 | the same index **with full signatures** - 4.2 MB - **[verified]**  |
| `GET /help?format=Full&target={fnName}` | one function's signature, cheaply - **[verified]**                 |

> Both `/help` and swagger bodies are **double-encoded**: the response is a JSON _string_ whose
> contents are the JSON document. Parse twice. But a plain-string return value (e.g.
> `/product-session/v1/logs/path/{p}`) is only encoded _once_ - an unconditional double-parse
> throws on those. Try the second parse, keep the string on failure.

**`/help?format=Full` is the authoritative index, and it is the one to use.** For every function
it gives argument names, argument types, and the return type:

```jsonc
{
  "name": "PostRiotclientappV1NewArgs",
  "arguments": [
    { "name": "args", "optional": false, "type": { "type": "vector", "elementType": "string" } },
  ],
  "returns": { "type": "", "elementType": "" },
}
```

Swagger is a strict **subset** - 1066 operations vs 1261 functions, and it is missing whole
namespaces we care about: `product-launcher`, `process-control`, `vanguard`, `patch` (most of it),
`rnet-product-registry`, `riotclientapp`, `patch-proxy`. Reading swagger alone is what hid the real
launch endpoint. Treat `/help` as the index and swagger only as a source of exact path spellings
for the routes it does cover.

**Deriving a path from a function name.** `/help` gives no paths. The convention is
`{Verb}{PascalSegments}` with `By{Param}` marking a path parameter:

```
GetPatchV1InstallsByInstallIdStatusPatch
  -> GET /patch/v1/installs/{install-id}/status/patch
PostProductLauncherV1ProductsByProductIdPatchlinesByPatchlineId
  -> POST /product-launcher/v1/products/{product-id}/patchlines/{patchline-id}
```

Word-boundary guesses go wrong often enough that a derived path must be probed before it is
trusted (`quit-switch-background-mode` vs `quit/switch-background-mode` cost a doc correction).
Probe with a `GET`; the reply distinguishes three states:

| Response                                 | Meaning                                                                                                      |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `405`                                    | route exists, wrong verb - **the spelling is right**                                                         |
| `404` `"errorCode":"RESOURCE_NOT_FOUND"` | no such route - wrong spelling, or plugin not registered                                                     |
| `404` `"errorCode":"RPC_ERROR"`          | route **is** registered, handler unavailable (plugin present but not ready / not applicable to this account) |

That last distinction is worth wiring into error handling: `RPC_ERROR` means retry later,
`RESOURCE_NOT_FOUND` means never.

**Argument encoding.** A function taking a single non-path argument expects the request body to
**be that value**, not an object wrapping it. `POST /riotclientapp/v1/new-args` wants a bare
`[]`; sending `{"args": []}` returns
`400 Couldn't assign value to 'args' of type vector because the input not a collection`.

---

## 1. Tier 1 - things we should actually wire up

### 1.1 Auto-detect the League install path **[verified]**

```
GET /data-store/v1/product-settings/products/league_of_legends/patchlines/live
```

```json
{
  "product_install_full_path": "C:/Riot Games/League of Legends",
  "product_install_root": "C:/Riot Games",
  "patching_policy": "manual",
  "auto_patching_enabled_by_player": false,
  "should_repair": false,
  "settings": { "locale": "en_US", "create_shortcut": false },
  "locale_data": { "default_locale": "en_GB", "available_locales": ["ar_AE", "…"] },
  "dependencies": { "vanguard": true, "Direct X 9": { "phase": "Succeeded" } }
}
```

This is the single highest-value find. Today `league_path` is user-configured and
`GameDir::resolve` has to guess whether they pointed at the root or at `Game/`. Riot tells us
authoritatively. **Proposal:** on first run (and as a "Detect" button next to the path field),
query this and pre-fill. Keep the manual override - the endpoint only works when the Riot
Client is running, and users with odd installs still need the escape hatch.

Corroborating, and less dependent on the product-settings shape:

```
GET /patch/v1/installs                                → ["league_of_legends.live",
                                                          "league_of_legends.live.game_patch"]
GET /patch/v1/installs/league_of_legends.live         → path: "C:/Riot Games/League of Legends"
GET /patch/v1/installs/league_of_legends.live.game_patch → path: "C:/Riot Games/League of Legends/Game"
```

Note the second one resolves **exactly** what `GameDir` wants - the `Game` directory, no
guessing. Both **[verified]**.

`patching_policy` and `auto_patching_enabled_by_player` are worth surfacing too: a user on
`automatic` will have League silently patch out from under a built overlay.

### 1.2 Patch state - gate the launch, detect content changes **[verified]**

```
GET /patch/v1/installs/league_of_legends.live/status
```

```json
{
  "patch": {
    "state": "up_to_date",
    "error": null,
    "progress": { "phase": "None", "progress": 0.0, "manifest": null, "update": null },
    "tags": ["en_US"],
    "url": "https://lol.secure.dyn.riotcdn.net/channels/public/releases/ED5FB7B738681EE8.manifest"
  },
  "preview": { "diff": { "out_of_date": false, "new_bytes": 0, "disk_size_diff": 0, … } },
  "repair":  { "state": "pending", "repair_progress": { … } },
  "seed":    { "state": "out_of_date" }
}
```

Two uses:

1. **Refuse to patch/launch mid-update.** Injecting into a client whose WADs are being
   rewritten underneath it is a guaranteed corruption report. `patch.state != "up_to_date"`
   or `progress.phase != "None"` → block with a clear message and a progress bar
   (`progress.progress` is 0..1).
2. **Invalidate the overlay cache on a new patch.** The manifest URL embeds a release id
   (`ED5FB7B738681EE8`) which also shows up as `version` in the session payload (section 1.3) and in
   `product-metadata`. Persist it; when it changes, force a full overlay rebuild instead of the
   incremental one. That is exactly the failure `rebuild_overlay` exists as a manual escape
   hatch for.

`preview.diff` is a _dry run_ - `PUT …/requests/preview` then read `out_of_date`,
`new_bytes`, `disk_size_diff` to answer "is a patch pending, and how big" **without**
downloading it. Good for a pre-launch warning.

`PUT /patch/v1/installs/{id}/requests/repair` triggers a real repair (and
`DELETE` cancels it). A "Repair game files" button is a genuinely useful support action -
but it restores every modded WAD, so it must be behind a confirmation that says so.

### 1.3 Session state - replace process polling **[verified]**

```
GET /product-session/v1/sessions          (all, incl. the host app)
GET /product-session/v1/external-sessions (games only - what we want)
```

```json
{
  "BUBZp9ccQI3KiSv-Uuw3Iw": {
    "productId": "league_of_legends",
    "patchlineId": "live",
    "patchlineFullName": "League of Legends",
    "version": "ED5FB7B738681EE8",
    "phase": "None",
    "exitCode": 0,
    "exitReason": null,
    "isInternal": false,
    "launchConfiguration": {
      "executable": "C:/Riot Games/League of Legends/LeagueClient.exe",
      "workingDirectory": "C:/Riot Games/League of Legends",
      "locale": "en_US",
      "arguments": [
        "--riotclient-auth-token=…",
        "--riotclient-app-port=49440",
        "--no-rads",
        "--disable-self-update",
        "--region=EUW",
        "--locale=en_US",
        "--riotgamesapi-standalone",
        "--riotgamesapi-settings=<base64>",
        "--rga-lite",
        "--subject=…"
      ]
    }
  }
}
```

This is strictly better than `list_running_league()` for the launcher's "did it start?" poll:

- It is **authoritative** - Riot's own bookkeeping, not a name match against a toolhelp snapshot.
- It carries `exitCode` / `exitReason`, so a League that started and immediately died gives us a
  reason to show instead of "nothing happened".
- It tells us the **patchline** (live vs pbe) and the **version** for free.
- The map key is the session id, which is also the `--riotclient-auth-token` value.

Keep the process scan as a fallback for the (rare) case where the Riot Client exits but League
keeps running.

`phase` is the field the game itself pushes via `POST /product-session/v2/heartbeat` - worth
watching to see whether it distinguishes lobby from in-game. Unverified; it read `"None"` for
the whole session, which may just mean League doesn't populate it.

### 1.4 Push events over WebSocket - stop polling entirely **(unverified)**

> **Partly wrong - see section 1.10 for the enumerated list.** This table was written from convention
> before the events were enumerated. Two of its rows name events that **do not exist**:
> `OnJsonApiEvent_product-session_v1_external-sessions` and
> `OnJsonApiEvent_product-metadata_v2_products`. Kept only for the transport notes below.

`/help` lists 61 events (not 100 - that count was the earlier misreading). The ones below are
**[verified]** present except where struck:

| Event                                                     | Fires on                                                                              |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| ~~`OnJsonApiEvent_product-session_v1_external-sessions`~~ | ❌ **does not exist** - use the `OnJsonApiEvent` firehose and filter                  |
| `OnJsonApiEvent_patch-proxy_v2_patch-states`              | patch progress                                                                        |
| `OnJsonApiEvent_patch-proxy_v2_install-states`            | install added / removed                                                               |
| `OnJsonApiEvent_rnet-product-registry_v4_patch-states`    | patch progress (registry view)                                                        |
| ~~`OnJsonApiEvent_product-metadata_v2_products`~~         | ❌ **does not exist** - nearest is `OnJsonApiEvent_rnet-product-registry_v4_products` |
| `OnJsonApiEvent_riotclientapp_v1_new-args`                | someone handed off launch args                                                        |

Connect `wss://127.0.0.1:{port}/` with the same Basic auth, then send the WAMP subscribe frame
`[5, "<eventName>"]`; events arrive as `[8, "<eventName>", {…}]`. This is the standard LCU
pattern and the event names match its convention, but **I did not open a socket to confirm it** -
treat the frame format as needing a spike before anyone builds on it.

If it works, the launcher's post-launch poll loop collapses into a subscription, and we get
"League closed" for free rather than discovering it on the next 2s tick.

---

### 1.5 The product-launcher - how you actually start a game **[verified]**

Missing from the original survey and from `league-launch-flow.md` section 4.4a, which cost a debugging
session. Found by reading the client's own `/help` index rather than trusting the earlier writeup.

```
GET  /product-launcher/v1/products/{productId}/patchlines/{patchlineId}/eligibility → true
POST /product-launcher/v1/products/{productId}/patchlines/{patchlineId}   {}        → 200 "<sessionId>"
```

The 200 body is a bare JSON string: the session id, which is the key into `external-sessions` (section 1.3).
So a launch hands you session tracking for free - no process-name polling needed.

`new-args` is **not** a launch API. Its 204 means "arguments queued" and a fully booted client acts
on none of them. Its only use is waking a tray-idle client (see `league-launch-flow.md` section 4.4c).

Other functions in the same namespace, unverified but named clearly enough to be worth knowing:

| Function                                          | Likely use                                  | Status                                                                               |
| ------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------ |
| `GetProductLauncherV1IsLaunchRequestPending`      | is a launch already in flight               | **[verified]** `false`; also the cheapest readiness probe (see below)                |
| `PostProductLauncherV1DefaultProductFocus`        | bring the client forward without `ASFW_ANY` | signature only                                                                       |
| `GetLaunchRestrictionV1Ready` / `…V1Restrictions` | launch bans - explains a silent no-op       | **404 `RPC_ERROR`** - route registered, plugin not ready. Retryable, not absent (section 0) |
| `DeleteRiotClientAppCommandV1LaunchRequest`       | cancel a pending launch                     | signature only                                                                       |

See section 1.9 for the rest of the namespace - including session recovery, clean shutdown, and a
per-patchline rogue-process allow-list.

**Two API-surface warnings for anything built on this file.**

1. **A tray-idle client is not a running client.** Re-measured 2026-07-27 against a genuinely idle
   client, which is thinner than first recorded: `/help` collapses to **1,277 bytes / 7 functions**,
   and five of those seven are remoting primitives (`Exit`, `Help`, `Subscribe`, `Unsubscribe`,
   `WebSocketFormat`). Only **two** real functions remain - `PostRiotclientappV1NewArgs` and
   `GetRiotclientappV1IsXbgpRunning`. Every namespace in this document 404s, `/swagger` included.
   "The lockfile parses and the pid is alive" does **not** mean the API is there.

   Note `GetRiotclientappV1IsXbgpRunning` is _listed_ while `/riotclientapp/v1/is-xbgp-running`
   still 404s: **being in `/help` does not mean the REST route is mounted.** Probe, don't infer.

2. **The port churns under a stable pid.** Waking the client restarts its remoting listener;
   observed 64699 → 60865 → 63057 → 61319 → 65157, pid 49768 throughout. Re-read the lockfile
   before every request rather than caching a port at session start.

**The wake→poll sequence, measured end-to-end** (2026-07-27, from genuine tray-idle):

```
POST /riotclientapp/v1/new-args   body []   → 204        # bare array, NOT {"args":[]}
t+1.0s   lockfile port 61319 → 65157   (pid 49768 unchanged)
t+2.6s   GET /product-launcher/v1/is-launch-request-pending → 200   # namespace registered
```

So the readiness signal is a `200` from any `product-launcher` route, reached in **under 3 s**.
A poll interval of ~250-500 ms with a 10 s ceiling is comfortable; the loop must re-read the
lockfile each iteration or it dies on the port change at t+1.0 s.

---

### 1.6 The product registry - one call that answers install, PBE, and Game-dir **[verified]**

> **Shipped 2026-07-27** as `ritoclient-api::product_registry`. The envelope was confirmed by
> probing a live client: a bare JSON array of products, each `{id, name, patchlines[]}`. Fields are
> snake*case \_except* `secondary_patchlines[].relativePath`, which is camelCase inside an otherwise
> snake_case object; the parser accepts both spellings. Re-run the probe with
> `cargo test -p ritoclient-api product_registry_probe -- --ignored --nocapture` - it dumps the raw
> response, which is the only way to see fields we don't model yet.
>
> Wired into install auto-detection. The `release_id` cache key is **not** wired - see section 4 item 1.

```
GET /rnet-product-registry/v4/products
```

Returns every product with all its patchlines. This single call replaces the install-path
detection in section 1.1, the `patch/v1/installs` string-splitting in `pbe-multi-install.md`, and the
hardcoded `Game` subdirectory. Measured, `league_of_legends`:

| Field                  | `live`                                        | `pbe`                     |
| ---------------------- | --------------------------------------------- | ------------------------- |
| `install_id`           | `league_of_legends.live`                      | `league_of_legends.pbe`   |
| `install_full_path`    | `C:/Riot Games/League of Legends`             | `""`                      |
| `install_dir`          | `League of Legends`                           | `League of Legends (PBE)` |
| `primary_executable`   | `LeagueClient.exe`                            | `""`                      |
| `release_id`           | `ED5FB7B738681EE8`                            | `""`                      |
| `configuration_status` | `has_configuration`                           | `unsupported_region`      |
| `vanguard_dependency`  | `true`                                        | `false`                   |
| `secondary_patchlines` | `[{"id":"game_patch","relativePath":"Game"}]` | `[]`                      |
| `launch_disabled`      | `false`                                       | `false`                   |

Three things fall out of this:

1. **Installed-ness test.** `install_full_path != ""` (equivalently `primary_executable != ""`).
   This is the check the PBE picker needs - **not** `…/eligibility`, which returns `true` for
   `pbe` on this machine, which has no PBE install. Eligibility is an entitlement check, not an
   install check, exactly like the locale fallback already flagged in `pbe-multi-install.md`.
2. **Stop hardcoding `Game`.** The game directory is
   `install_full_path + "/" + secondary_patchlines[id=="game_patch"].relativePath`. It is a
   declared relative path, so derive it rather than assuming.
3. **`release_id` is right here**, so the overlay-cache key from section 1.2 needs no second call - and
   it is per-patchline, which is exactly the widening `pbe-multi-install.md` section 3.3 asks for.

`GET /rnet-product-registry/v1/install-states` is the cheap version when only installed-ness
matters: `[{"id":"league_of_legends","has_installed_patchline":true,"patchline_install_states":
{"live":"installed","live.game_patch":"installed"}}]`.

### 1.7 The patcher's own exclusion list - which files survive a patch **[verified]**

```
GET /patch/v1/installs/{install-id}          → .excluded_paths
```

Identical for `league_of_legends.live` and its `.game_patch` child, and relative to the install
root `C:/Riot Games/League of Legends`:

```
Config  Cookies  DATA  GPUCache  Game  Logs  RADS  Update  Saved  Screenshots  TFT
debug.log  Game.db  Game.db-journal  Game.manifest  Game.ok
LeagueClient.db  LeagueClient.db-journal  LeagueClient.manifest
lockfile  lockfile_  patchline.json  SOFT_REPAIR  installation_guid  Uninstall*
```

This is the patcher telling us, in its own words, which paths it will not touch. Directly
relevant to where mod artifacts are safe to stage. Note it is a **root-relative** list, so
`DATA` here is `<root>/DATA`, not the `Game/DATA` the WADs live in - do not read this as "the
game's DATA folder is patch-exempt", because it is not. Verify against a real patch before
relying on any entry.

### 1.8 Vanguard state - the pre-launch gate we were missing **[verified]**

```
GET /vanguard/v1/status
→ {"status":"UpToDate","enabled":true,"version":"1.18.4.47","available":"1.18.4.47",
   "restartRequired":false,"restartWaivedReason":"","progress":null}
```

`restartRequired` is the interesting field: Vanguard can demand a **reboot** before a launch will
succeed, and a launch attempted in that state fails in a way that looks like our bug. Combined
with `vanguard_dependency` from section 1.6 this is a clean precondition check. Absent from swagger.

`POST /vanguard/v1/check-vanguard-service` re-checks the service, and
`PUT /vanguard/v1/update/products/{p}/patchlines/{pl}` triggers an update **prompting UAC** -
don't call either without the user asking.

### 1.9 Launcher-side controls worth knowing **[unverified - signatures only]**

From `/help?format=Full`, all in `product-launcher` / `process-control`, none in swagger:

| Function → route                                                                               | Why it matters                                                                                                                                                                                 |
| ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PUT /product-launcher/v1/products/{p}/patchlines/{pl}` `(pid)`                                | **"Recover a session for a product that is already running, but Riot Client Services doesn't know about."** Re-attaches RCS to a game it didn't start. The single most interesting call found. |
| `DELETE /product-launcher/v1/products/{p}/patchlines/{pl}` `(shouldTerminateProcess: bool)`    | Close the launched game cleanly - a real "Stop Game" button, instead of killing a pid                                                                                                          |
| `DELETE /product-launcher/v1/products/{p}/patchlines/{pl}/rogue-process/{name}/{pid}`          | There is a **`rogue_process_allow_list`** per patchline (empty for us). An allow-list for processes running alongside the game is worth understanding before we run one.                       |
| `POST /product-launcher/v1/default-product/focus` / `/minimize` / `/flash`                     | Window control of the launched game without `AllowSetForegroundWindow`                                                                                                                         |
| `PUT /rnet-product-registry/v4/session-patch-lock/products/{p}/patchline/{pl}` `(isExclusive)` | **A patch lock.** Plausibly the sanctioned way to stop the client patching over staged mods. Highest-value unknown here.                                                                       |
| `POST /process-control/v1/process/client-restarting`                                           | "Sets state that client is restarting - do not quit". Holds the Riot Client alive.                                                                                                             |
| `GET /process-control/v1/process`                                                              | `{"pid":49768,"status":"Running","restart-countdown-seconds":null}` - **[verified]**                                                                                                           |
| `PUT /patch/v1/installs/{id}/requests/repair`                                                  | Trigger a repair - restores modded files. `DELETE` interrupts one in progress.                                                                                                                 |
| `PUT /rnet-product-registry/v4/products/{p}/patchlines/{pl}/root-path` `(path)`                | Rewrites where the client thinks the install lives. Powerful and dangerous.                                                                                                                    |

⚠️ The bottom half of that table **mutates client or install state**. `DELETE /patch/v1/installs/{id}`
deletes an install resource outright. Nothing here should be called speculatively - and the two
marked dangerous should probably never be called by us at all.

#### The Riot Client's own window **[verified 2026-07-27]**

Note that `/product-launcher/v1/default-product/*` above controls the **launched game's** window.
The Riot Client's own window is a different plugin, and it was probed and called:

| Route                                                        | Description (from `/help`)                                        | Status                          |
| ------------------------------------------------------------ | ----------------------------------------------------------------- | ------------------------------- |
| `POST /riot-client-lifecycle/v1/hide`                        | "Hide the UX." No arguments.                                      | **called, 2xx** - shipped       |
| `POST /riot-client-lifecycle/v1/show`                        | "Show the UX." No arguments.                                      | **called, 2xx**                 |
| `POST /riot-client-lifecycle/v1/minimize`                    | -                                                                 | **404 - does not exist**        |
| `POST /riot-client-lifecycle/v1/quit`                        | -                                                                 | 405 on GET (exists), not called |
| `POST /riot-client-lifecycle/v1/quit/switch-background-mode` | "...If any games are running show the games-running exit-dialog." | 405 on GET (exists), avoid      |

Three things fell out of probing this:

1. **There is no `minimize` for the Riot Client.** `hide` sends the window to the tray; it does not
   go to the taskbar. Any UI wording promising "minimize" would be wrong.
2. **`quit` is not a tidier `hide`.** League holds a live remoting session with the client for its
   entire run, so quitting it out from under a running game is a different and worse thing.
3. **`quit/switch-background-mode` is the wrong call here** even though it produces the tray-idle
   state, because its own description says it raises the games-running exit dialog when a game is
   up - which is precisely when we would be calling it.

Shipped as `ritoclient-api::lifecycle`, behind the `hideRiotClientOnLaunch` setting (default on -
leaving the launcher parked on the desktop behind the game is the worse default).
The hide is deferred to a background thread that waits for the game process, because at the moment
the launch request is accepted the game is not up yet - and on a cold start it is minutes away.

**Re-probed 2026-08-19** against a booted 137.0.3.4826 (1275 functions in `/help`), looking for a
"stay hidden" lever that would make the deferred hide and its re-assert loop unnecessary:

| Route                                                     | Description (from `/help`)                                                          | Status                                                        |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| `GET/PUT/DELETE /riot-client-lifecycle/v1/ux-command`     | DELETE: "Deletes the current UX command so that the default client UX app no longer needs to process it" | GET **called**: 404 `"No command found."` while nothing pends |
| `PUT /rnet-lifecycle/v1/hide` / `quit` / `restart`        | `quit`: "Quit Riot Client. If any games are running hide Riot Client instead."       | present, not called                                             |
| `GET /rnet-lifecycle/v1/product-context-phase`            | The phase of the lifecycle path's product context                                    | **called**: 404 "Product context not available yet"            |

Three things fell out of this probe too:

1. **No "stay hidden" or "launch hidden" function exists.** All 1275 names and descriptions were
   swept for hide/show/window/tray/visible/minimize/foreground. The client's own window has `hide`
   and `show` in two spellings (`riot-client-lifecycle` v1 POST, `rnet-lifecycle` v1 PUT), and
   nothing else.
2. **The game-exit un-hide is a UX command, and the pending command is addressable.** The
   `UxCommand` type is `{ action, parameters, showUxIfHidden: bool }` - the very flag the re-hide
   loop exists to fight (actions: `ShowLogin`, `ShowAllProducts`, `ShowProduct`, `ShowSettings`,
   `PassFocusPermissionToFoundation`, `Test`, `ShowModal`). `DELETE .../ux-command` removes the
   pending command before the UX processes it, which would suppress the un-hide at its source
   instead of racing it with re-hides. Untested against a real game exit - the delete may lose the
   same race the re-hide does.
3. **`product-context-phase` is not a "game up" signal for us.** It answers only on the lifecycle
   launch path, which our launches deliberately do not walk. The session record stays the honest
   "did it start?" source for the `product-launcher` route.

### 1.10 Push events - the real list **[verified]**

61 events, subscribed over WSS with `[5, "OnJsonApiEvent_..."]`. The ones that matter:

| Event                                                          | Replaces                                   |
| -------------------------------------------------------------- | ------------------------------------------ |
| `OnJsonApiEvent_product-launcher_v1_is-launch-request-pending` | polling for launch-in-flight               |
| `OnJsonApiEvent_process-control_v1_process`                    | Riot Client lifecycle / restart countdown  |
| `OnJsonApiEvent_vanguard_v1_status`                            | anti-cheat state changes (section 1.8)            |
| `OnJsonApiEvent_rnet-product-registry_v4_patch-states`         | patch progress - the section 1.2 gate, pushed     |
| `OnJsonApiEvent_rnet-product-registry_v1_install-states`       | install appears/disappears (PBE installed) |
| `OnJsonApiEvent_riot-client-app-command_v1_launch-request`     | something else requested a launch          |
| `OnJsonApiEvent`                                               | firehose - everything, including sessions  |

Note there is **no** dedicated `product-session_v1_external-sessions` event: to get "League
exited" pushed, subscribe to the `OnJsonApiEvent` firehose and filter.

---

## 2. Tier 2 - useful, lower priority

| Endpoint                                                          | Value                                                                      | Status                                                                             |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `GET /product-integration/v1/locale/products/{p}/patchlines/{pl}` | League's locale as a plain string (`"en_US"`) - no YAML parsing            | **[verified]**                                                                     |
| `PUT` on the same path                                            | _Set_ League's locale                                                      | unverified                                                                         |
| `GET /riotclient/region-locale`                                   | `{"locale":"en_US","region":"EUW","webLanguage":…,"webRegion":…}`          | **[verified]**                                                                     |
| `GET /product-metadata/v1/definitions/products`                   | Every entitled product + its patchlines - `league_of_legends: [live, pbe]` | **[verified]**                                                                     |
| `GET /patch-proxy/v2/install-states`                              | Which products are installed at all (`{"live":"installed"}`)               | **[verified]**                                                                     |
| `GET /product-session/v1/logs/path/{patchline-name}`              | League's log directory, for diagnostics bundles                            | unverified                                                                         |
| `GET /product-session/v1/data/path/{patchline-name}`              | League's data directory                                                    | unverified                                                                         |
| `GET /riotclient/system-info/v1/basic-info`                       | OS, RAM, CPU cores - diagnostics                                           | unverified                                                                         |
| `GET /riotclient/command-line-args`                               | Args the running Riot Client got (`[]` here)                               | **[verified]**                                                                     |
| `GET /launch-restriction/v1/restrictions`                         | Player launch bans - would explain a launch that silently no-ops           | **404** on this client (plugin not loaded when logged out of the relevant product) |

The locale ones deserve a note: `ltk-manager-core/src/utils/locale.rs` currently sniffs the
locale off disk. These endpoints are more direct, but only work while the Riot Client runs, so
they're a _supplement_ to the disk path, not a replacement.

`product-metadata` giving us `[live, pbe]` is what would let us offer a PBE launch target
without hardcoding patchline names - `LaunchTarget.patchline_id` already models this.

---

## 3. Tier 3 - interesting, but leave alone

- **`client-feature-flags/v1/debug/overrides/flags/{namespace}/{flag}`** (PUT/DELETE) lets you
  forcibly set any client feature flag, and `…/debug/mock-mode` fakes client-config wholesale.
  Tempting for enabling unreleased client features. It's a debug surface on the _Riot Client_,
  not the game, so it can't unlock anything we care about, and writing to it puts the client in
  a state the user didn't ask for. Not worth the support burden.
- **`GET /client-config/v2/namespace/{namespace}`** reads the same `keystone.products.*` config
  the launch template comes from - useful for _research_ (see the clientconfig experimental-modes
  work), not for the launcher.
- **`PUT /app-command/submit`** and `/app-command/v1/focus-request` - coerce the client to run a
  registered command / request focus. The focus half might matter if we ever need to bring the
  client forward after launching, but `AllowSetForegroundWindow` is the sanctioned route.
- **`/rso-auth`, `/rso-authenticator`, `/player-account`, `/entitlements`, `/payments`** - 75+36+33
  paths of authentication and commerce. Out of scope, permanently. We do not touch credentials.
- **`POST /product-integration/v1/settings-token`** mints the `--riotgamesapi-settings` blob for a
  standalone SDK instance. This is the credential-handoff mechanism that makes launching
  `LeagueClient.exe` directly _look_ feasible. It is not our business, and the "Scope" section of
  `league-launch-flow.md` still stands: **we do not spawn `LeagueClient.exe` ourselves.**

---

## 4. Suggested next steps

Reordered after the 2026-07-27 full-`functions` pass.

1. ◐ **Switch install detection to `/rnet-product-registry/v4/products`** (section 1.6). One call now
   yields install path, Game subdirectory, `release_id`, PBE presence and Vanguard dependency -
   collapsing section 1.1, the PBE discovery design, and the section 1.2 cache key into a single source. Do
   this before building the `GameInstall` list in `pbe-multi-install.md`, since it changes the
   shape of that work.

   Detection ✅. **Still open: the `release_id` overlay cache key.** It needs a decision first -
   the registry only answers while the client is running, so keying the cache on `release_id`
   naively means the key silently changes shape when the client is closed, and every build with
   no client would flush the previous one. Storing "last known release id" and flushing only on a
   _known-and-different_ id avoids that, but it decides where the fetch happens: inside the
   overlay build (a blocking HTTP call in the build path) or once at patcher start.

2. ✅ **Fix the PBE installed-ness test** to `install_full_path != ""`. `…/eligibility` returns
   `true` for an uninstalled `pbe` - a live trap, not a hypothetical one.
   `Patchline::is_installed` is that test; `is_eligible`'s doc comment now says outright that it
   is not.
3. **Add the Vanguard precondition** (section 1.8). `restartRequired` produces launch failures that look
   exactly like our bugs; one GET rules it out.
4. **Spike the WebSocket** (section 1.10) - the real event names are now known. It's the one item that
   changes the design rather than adding a feature: if subscriptions work, the launcher polls
   nothing.
5. **Verify `check the wake→poll loop`** against the measured 2.6 s registration in section 1.5, and
   confirm `wake_with_launch_args` sends a bare `[]` body rather than `{"args": […]}`.
6. **Investigate the session patch lock** (section 1.9) - if
   `PUT …/session-patch-lock/…?isExclusive=true` does what its name says, it is the sanctioned
   answer to "client patched over my mods", which we currently have no answer for at all.
7. **Add the patch gate** (section 1.2) to the launch flow, with a new `ErrorCode::LeaguePatchInProgress`.

Items 1-3 and 7 are additive to the flow in `league-launch-flow.md` and don't invalidate any of
it. Item 6 is genuine research and may come back empty.

---

## 5. Caveats

- Every endpoint here requires the **Riot Client to be running**. Cold-start paths must keep
  their on-disk fallbacks.
- Riot rewrites these namespaces between patches. `rnet-product-registry` (v1/v4),
  `patch-proxy` (v1/v2) and `product-session` (v1/v2) all show visible version churn, and
  several documented paths 404 on a live client. Treat every call as best-effort: a failure
  should degrade to the existing on-disk behaviour, never fail a launch.
- Values captured above are from one machine (EUW, `C:/Riot Games`, `en_US`). Paths use forward
  slashes even on Windows - normalize before handing to `PathBuf`.
- **Never log the lockfile password**, and note that `/product-session/v1/sessions` returns the
  `--riotgamesapi-settings` blob, which contains an RSO authorization key. If we ever log a
  session payload for diagnostics, **strip `launchConfiguration.arguments`** first.

---

## 6. Appendix - the full namespace map

1261 functions across 126 namespaces (2026-07-27). Recorded so a future pass can go straight to
the relevant one instead of re-deriving this. **Bold** = covered in this document.

| Namespace                 | n   | Namespace                 | n   | Namespace                | n   |
| ------------------------- | --- | ------------------------- | --- | ------------------------ | --- |
| `Chat`                    | 100 | `Eula`                    | 14  | `Presences`              | 8   |
| `RsoAuthenticator`        | 87  | `PlaystationAccount`      | 14  | **`ProcessControl`**     | 8   |
| `VoiceChat`               | 72  | **`RiotClientLifecycle`** | 14  | `ProductIntegrationDeps` | 8   |
| `RsoAuth`                 | 48  | `XboxAccount`             | 14  | `FriendsForever`         | 7   |
| `Social`                  | 42  | `PayMobile`               | 14  | `GaWarning`              | 7   |
| _(non-REST)_              | 37  | `IntegrationTest`         | 13  | `Payments`               | 7   |
| **`RnetProductRegistry`** | 36  | `PlayerSessionLifecycle`  | 13  | `RnetSanitizer`          | 7   |
| `RsoMobileUi`             | 33  | `RnetLifecycle`           | 13  | `Tracing`                | 7   |
| `Chatbox`                 | 27  | `Agent`                   | 12  | `RnetPft`                | 7   |
| `PlatformUi`              | 25  | `PlayerReporting`         | 12  | `Vng`                    | 7   |
| `RiotMessagingService`    | 25  | **`DataStore`**           | 11  | `Localization`           | 6   |
| `ProductIntegration`      | 22  | `PlayerAccountAliases`    | 11  | `NetworkConnectivity`    | 6   |
| `PatchProxy`              | 21  | `ClientFeatureFlags`      | 10  | `PluginManager`          | 5   |
| `ActivityGateway`         | 19  | `Mailbox`                 | 10  | `ProductUpdateScanner`   | 5   |
| **`Patch`**               | 19  | `PlayerPreferences`       | 10  | `RiotStatus`             | 5   |
| `ClientConfig`            | 18  | `Permissions`             | 10  | `RnetSelfUpdate`         | 5   |
| **`ProductSession`**      | 18  | `Telemetry`               | 9   | **`Vanguard`**           | 5   |
| `GaRestriction`           | 17  | `GameSession`             | 8   | **`Riotclientapp`**      | 5   |
| **`ProductLauncher`**     | 16  | `ProductMetadata`         | 16  | `PlatformSocial`         | 15  |

Remaining single-digit namespaces: `Aes`, `AgeRestriction`, `AntiAddiction`, `AppCommand`,
`AppleAccount`, `CookieJar`, `DeleteAccount`, `DiscordAccount`, `Entitlements`,
`FacebookAccount`, `FirstPartyFulfillment`, `GameActivity`, `GamecenterAccount`, `GoogleAccount`,
`InfoRadiator`, `JwtAuthenticator`, `KrAccountConfig`, `KrAccountPromotion`,
**`LaunchRestriction`**, `Loyalty`, `MobileProductRegistry`, `ModeManager`, `MsdkAccount`,
`NativeUx`, `OktaAccount`, `PlatformLogin`, `PlatformNotifications`, `PlayerAccount*` (7),
`PlayerAffinity*` (3), `PlayerBehaviorToken`, `Privacy`, `PublishingContent`, `RcAuth`,
`RcInfoRadiator`, `Reference`, `Restriction`, `RiotFriends`, `RiotLogin`, `RiotclientSystemInfo`,
`SignificantChange`, `StartupConfig`, `Swagger`, `SystemInfo`, `SystemTray`,
`VanguardSessionManager`, `Commerce`, `ExternalMessageHandler`, `PrivateSettings`,
`RiotClientAppCommand`, `Riotclient`, `ProductLocalization`, `RiotClientAuth`,
`RiotClientLifecycleState`, `RsoAuthConfiguration`, `Scd`, `TencentLauncher`, `KeypairAccount`,
`MobilePush`, `NintendoAccount`.

Roughly 300 of the 1261 are authentication (`RsoAuth*`, `PlayerAccount*`, third-party account
linking) and another ~250 are social (`Chat`, `VoiceChat`, `Social`, `Chatbox`). Both are
permanently out of scope - the launcher-relevant surface is the ~150 functions in the bolded
namespaces.
