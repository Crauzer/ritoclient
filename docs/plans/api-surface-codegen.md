# Plan: generating the API surface

How this crate gets from four hand-written namespaces to the client's full in-scope surface,
generated from the client's own self-description.

Source data: [`../riot-client-local-api.md`](../riot-client-local-api.md). Layout rationale and the
one blocking decision: [`../design/endpoint-layout.md`](../design/endpoint-layout.md).

## Where we are

| Step                         | State                                                              |
| ---------------------------- | ------------------------------------------------------------------ |
| 1. Harness                   | **Done.** `client`, `retry`, `route`, `namespaces/`, `models/`      |
| 1½. Shape + layering         | **Done, 2026-07-29.** Endpoint-as-a-type, built and shipped: three crates, `Endpoint`/`EndpointBuilder` in core, all four namespaces converted, `EndpointMeta` enumeration. See [`../design/endpoint-layout.md`](../design/endpoint-layout.md) |
| 2. Snapshot                  | **Done, 2026-07-29.** `cargo xtask ritoclient-snapshot`; `schema/` holds 157 functions, 124 types, 38 swagger paths, and a probe ledger, taken against build 135.0.7.4760 |
| 3. Codegen                   | **Generator built and passing its fixture, 2026-07-29.** `cargo xtask ritoclient-codegen` wipes and rewrites `ritoclient-api/src`, reproducing the four hand-written namespaces byte-for-byte from the snapshot plus `overrides.toml`; CI fails on drift. Remaining: extend `overrides.toml` to the other seven in-scope namespaces |
| 4. Re-base curated modules   | **Done by construction.** The measured doc comments moved verbatim into `overrides.toml` and the fixture diff being empty is the proof nothing was dropped |
| 5. Rewrite the `lib.rs` preamble | Not started                                                    |

Step 1 shipped as this repo's first commit. What it built is documented by the code, the crate
README, and `CONTRIBUTING.md`; it is not restated here.

## Framing

The Riot Client's remoting server is **a local HTTP server we have full credentials for**. It is not
a secure enclave and the crate does not act like one:

- Routes are not withheld, gated, or graded by how dangerous they look. If the client exposes it and
  it is in scope, we generate it.
- Status codes are **data, not errors**. A 404 is an answer. Deciding what it means is the caller's
  job, because it differs per route - a 404 from `product-launcher` on a tray-idle client means
  "wait"; the same 404 from a read query means "give up".
- The one genuine constraint is not about safety but about secrets: never log the lockfile password,
  and `/product-session/v1/sessions` returns an RSO authorization key inside
  `launchConfiguration.arguments`, so a diagnostics dump strips that field. That is a logging rule,
  not an access-control policy.

Two layers, both public. **Low-level** `Client` builds any request against any path. **High-level**
generated typed endpoints per namespace, with the curated modules above them carrying orchestration
and measured knowledge.

## In scope

The eleven namespaces the survey bolded, ~150 of the client's 1261 functions:

`ProductLauncher` · `RnetProductRegistry` · `Patch` · `PatchProxy` · `ProductSession` ·
`ProcessControl` · `Vanguard` · `Riotclientapp` · `RiotClientLifecycle` · `DataStore` ·
`LaunchRestriction`

A scoping decision about generation effort and repo weight, **not** a safety boundary. The ~300 auth
and ~250 social functions are excluded because we have no use for them, and the low-level `Client`
reaches them anyway if that changes.

## Decisions taken

| Question          | Decision                                                                  |
| ----------------- | ------------------------------------------------------------------------- |
| Surface scope     | The eleven namespaces above                                               |
| Codegen delivery  | `xtask` reads a checked-in snapshot, writes checked-in `.rs`. No `build.rs` |
| Layering          | Generated layer is public; curated modules wrap it as the recommended path |
| Mutating routes   | Emitted like any other. No denylist, no markers, no ceremony               |
| Unprobed paths    | Emitted callable. Existence is asked at runtime (`Client::probe`), never recorded in the tables |
| 404 handling      | The caller's, by default                                                   |
| Retry             | Configurable policy on the `Client`, overridable per request               |
| Route versioning  | Per route, never per namespace (see below)                                 |
| Namespace layout  | Folder per namespace, `routes.rs` always split out                         |

### Version belongs to the route, not the namespace

On a live client, `/rnet-product-registry/v1/install-states` and `/rnet-product-registry/v4/products`
are **both current** - different resources, different versions, same namespace. `patch-proxy`
(v1/v2) and `product-session` (v1/v2) split the same way. There is no such thing as "the version of
`rnet-product-registry`", so a namespace-level version field would be wrong on first contact with a
real client.

**Nothing is negotiated.** A route names one version and the client asks for exactly that. No
fallback list, no probing, no resolution cache - if Riot retires a version the route 404s and the
caller decides. Version drift is caught by re-taking the snapshot and reading the diff, which is the
mechanism step 2 exists for. Note the survey documents versions *coexisting*, not a resource that
moved, so runtime fallback would be guessing at a problem not yet observed.

## Step 2 - Snapshot

`cargo xtask ritoclient-snapshot`. Needs a live client. Built and run
2026-07-29; three things it learned on first contact:

- **A tray-idle client has to be woken first** - its surface collapses to the
  argv handoff plus the remoting builtins (8 functions), so the snapshot sends
  `new-args` with an empty argv (opens the window, launches nothing) and polls
  `/help` until the plugins register.
- **Argument names come in two spellings** - `product-id` under
  `product-launcher`, `productId` under `data-store` and friends - so the
  `By{Param}` matcher normalizes both through one word split.
- **The parameterless derived paths were GET-probed**: 39 serving, 10
  registered, 28 absent. The absents are the known segment-split ambiguities
  (`logs/path`, `process/quit`, `quit/switch-background-mode`) plus the
  documented `is-xbgp-running` ghost - exactly what swagger spellings and
  `overrides.toml` `resource` entries exist to correct.

```
schema/                   repo root, not inside a published crate
  help.filtered.json      /help?format=Full, in-scope namespaces + transitive type closure
  openapi.filtered.json   /swagger/v3/openapi.json, same restriction
  probes.json             ledger: path, verb, observed status, date, client build
  overrides.toml          confirmed spellings and hand-authored corrections
  SNAPSHOT.md             provenance: date, client build, region
```

Full `/help?format=Full` is 4.2 MB with 3966 types; filtered to eleven namespaces and the transitive
closure of the types they reference it should be a small fraction, and reviewable in a diff. It sits
at the repo root rather than inside `ritoclient-api` so that `cargo package` does not ship the
generator's input to consumers who only want its output.

**Both bodies are double-encoded**: the response is a JSON *string* containing the JSON document, so
parse twice - except plain-string return values, encoded once. Try the second parse, keep the string
on failure.

### Paths have to be derived

`/help` gives no paths. The convention is `{Verb}{PascalSegments}`, with `By{Param}` marking a path
parameter:

```
GetPatchV1InstallsByInstallIdStatusPatch
  -> GET /patch/v1/installs/{install-id}/status/patch
```

Derivation is imperfect - `quit-switch-background-mode` vs `quit/switch-background-mode` cost a doc
correction, and `GetRiotclientappV1IsXbgpRunning` is listed in `/help` while the route 404s. So
**swagger's spelling wins where it has one, a recorded probe wins next, and a derived path is used
otherwise.** All three are emitted callable; probe evidence rides along as a doc line and stays in
`probes.json`, never as a field in the emitted tables - whether a route exists on a given client
is `Client::probe`'s question at runtime, because the answer changes per build and per boot state.
Withholding an unconfirmed path would be the enclave reflex again - if it is wrong it 404s, which
is the caller's to handle.

## Step 3 - Codegen

`cargo xtask ritoclient-codegen`. Offline, reads `schema/`, writes checked-in `.rs`. CI runs it and
fails on `git diff --exit-code`, catching hand-edited generated files.

The endpoint shape is settled - endpoint-as-a-type, decided and built 2026-07-29. The reasoning is
in [`../design/endpoint-layout.md`](../design/endpoint-layout.md), and the workspace split that
made the conversion mechanical is in [`../design/layering.md`](../design/layering.md). Deciding the
shape after the generator existed would have been the expensive version.

### The acceptance fixture

The four hand-written namespaces (`app_args`, `lifecycle`, `product_launcher`,
`product_registry`) are written the way a program would write them, which makes them the
generator's acceptance fixture: `cargo xtask ritoclient-codegen` run against the snapshot has to
reproduce all three files of each, and a diff is a bug in the generator rather than a decision to
make. Reproducing `mod.rs` means reproducing its doc comments, so the fixture also proves out
`overrides.toml` - on the namespaces whose measured prose we most want to keep, while it is still
small enough to move by hand.

Endpoint names follow the client's own function names with the verb and namespace prefix stripped:
`GetProductLauncherV1IsLaunchRequestPending` → `IsLaunchRequestPending`. They are module-scoped, so
cross-namespace collisions are free, and the convention was settled at 7 endpoints rather than 150.

**The fixture passed 2026-07-29** with one deliberate divergence: the doc line on the
`the_metadata_rows_match_their_endpoint_impls` test said the tables were "hand-maintained until
the generator writes them", which stopped being true at that exact moment; the regenerated wording
is the new checked-in text. Byte fidelity comes from the generator running `cargo fmt` on its own
output - the same formatter CI enforces - rather than from the emitters imitating rustfmt.

### Models: flat storage, grouped API

The split is what makes generation tractable:

- **`models::flat` is storage** - private, one flat namespace, every type under the client's own
  qualified name (`RnetProductRegistryProduct`). Generator output.
- **The grouping modules are the API** - public, per namespace, re-exporting from `flat` under
  ergonomic names (`models::product_registry::Product`).

Flat because the client's type universe is flat: `/help`'s 3966 type names are already globally
unique, and a type is routinely referenced by several namespaces. Emitting each once means
generation never has to decide which namespace *owns* a shared type - the grouping modules
re-export it into every group that uses it, rather than duplicating it or assigning an arbitrary
owner.

**Behaviour lives above the crate, not beside the types.** The grouping modules are generated too -
a re-export list is derivable from the schema plus a naming rule, and nothing in `ritoclient-api`
is hand-written. The methods that sit in them today (`Patchline::is_installed`, `secondary_dir`)
can neither stay through a wipe nor be re-homed as inherent impls in another crate (E0116). They
become extension traits in the facade - `PatchlineExt`, `ProductExt` - re-exported from
`ritoclient::prelude` beside `ClientExt`: same mechanism, same one-`use` cost, and the recorded
fixtures and tests move with them. See the data-never-code rule in
[`../design/layering.md`](../design/layering.md).

Serde tolerance (`#[serde(default)]`, unknown keys ignored) is a **generator policy** rather than a
per-type decision. Judgements a schema cannot supply - like `SecondaryPatchline`'s
`alias = "relative_path"`, insurance for the day Riot normalises that camelCase outlier - come from
`overrides.toml`.

### Where generated code lands

**All of `crates/ritoclient-api`.** `xtask` wipes `src/` and rewrites it; there is no protected file
inside, and `Cargo.toml` is the crate's only hand-written line of defence - a dependency list that
does not name `windows-sys` or the launcher. The folder-per-namespace layout still holds, and for the
same reason it always did: a layout that changes shape at some size threshold cannot be a generator
target. What changed is that `mod.rs` is now generated too. See
[`../design/layering.md`](../design/layering.md).

## Step 4 - Re-base the curated modules

`product_launcher`, `product_registry`, `lifecycle` and `app_args` keep **every doc comment and
fixture**, and their bodies go thin. The doc comments are the record of what was measured against a
live client; they exist nowhere else and do not get rewritten in passing - they move verbatim into
`schema/overrides.toml`, which is the only place a regenerated crate can keep them. Their launcher
half is already gone by then, hoisted into `ritoclient` by the workspace split
([`../design/layering.md`](../design/layering.md)) - and so are the model `impl` blocks, re-homed
as the facade's extension traits. This step confirms nothing measured was dropped; it moves no
code.

## Step 5 - Rewrite the `lib.rs` preamble

Layering, and the "does not do" list cut down. Most of it was posture; one item is a technical fact
and survives: we do not spawn `LeagueClient.exe`, because its argv carries an
`rso_auth.authorization-key` blob only an authenticated Riot Client can mint. See
[`../launch-protocol.md`](../launch-protocol.md).

## Decisions taken since

Nothing blocks step 3 any more.

| Question              | Decision                                                                     |
| --------------------- | ----------------------------------------------------------------------------- |
| Endpoint shape        | **Endpoint-as-a-type.** See [`../design/endpoint-layout.md`](../design/endpoint-layout.md) |
| Endpoint naming       | The client's function name, verb and namespace prefix stripped (`…V1IsLaunchRequestPending` → `IsLaunchRequestPending`). Module-scoped, so collisions are free |
| Pagination            | **No combinator.** Nothing observed on the local API pages; the `gitlab` crate's `api::paged` has no counterpart here |
| What is generated     | **All of `namespaces/`** - handlers included. No protected file inside it      |
| Measured doc comments | `schema/overrides.toml`, emitted by the generator. Single home                 |
| Crate layout          | **Three crates.** `ritoclient-core` (transport) ← `ritoclient-api` (generated) ← `ritoclient` (launcher + facade). Cargo's refusal of a cycle is the boundary; `xtask` is a fourth member, unpublished. See [`../design/layering.md`](../design/layering.md) |
| Crate name            | `ritoclient` is what downstream depends on, matching the repo                 |
| Snapshot provenance   | **Check `schema/` in**, filtered. The drift diff and CI regeneration both need the generator's input in the repo |
| Path placeholders     | **camelCase** - `{productId}`, not `{product-id}`. Never reaches the wire, but the generator and the fixture have to agree |
| Query parameters      | **No trait hook.** section 0 documents path params via `By{Param}` and everything else in the body; confirm against `arguments[].optional` at snapshot time |
| Model behaviour       | **Extension traits in the facade** (`PatchlineExt`, `ProductExt`), prelude re-exports. `overrides.toml` carries data - prose, aliases, spellings, probe notes - never code |
| Wire vocabulary       | **`Method` and `StatusCode` are `ritoclient-core`'s own types.** reqwest appears in no public signature, so the HTTP client is swappable without touching the generated crate or its callers |

**Still open, and not a generation concern:**

- **The `ts` feature.** Broken-by-design for external consumers and costing the downstream repo real
  bindings today. All six types it derives on are launcher types, so generation never touches them -
  this is a pre-*publish* decision. See [`../consumers.md`](../consumers.md), which recommends
  dropping it.
- **Testing generated code.** Not unit-testable without a live client. The probe ledger is the
  record that a route exists; recorded-response fixtures stay in the curated layer where the parsing
  lives.

## Deferred

- ~~**Detect a running game via `/product-session/v1/external-sessions`.**~~ Done as P3 of
  [`launch-flow-136.md`](launch-flow-136.md). The namespace is generated, `LaunchOutcome.session_id`
  is populated on every route that has one, and `SessionExt` reads the two enum fields. The process
  scan stayed as the fallback for the case survey section 1.3 flags - the Riot Client exits while the
  game keeps running.
- **WebSocket / the 61 push events.** Would collapse polling into subscriptions, but adds a socket
  and a background thread to a crate whose constraint is "blocking, not async". Event names are in
  survey section 1.10.
- **`--launch-background-mode`** on the cold-start path. A behaviour change.
