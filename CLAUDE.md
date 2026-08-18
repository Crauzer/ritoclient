# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

## What this is

A Rust workspace of three published crates - a client for the Riot Client's
local loopback API. Apache-2.0. Dependencies point downward only:

```text
crates/ritoclient/        orchestration + facade. What downstream depends on
crates/ritoclient-api/    typed namespaces and models. The generator's crate
crates/ritoclient-core/   transport: client, retry, route, lockfile, processes
```

## Status: experimental

Under heavy, early development. **Breaking changes are expected** - a Riot Client
update can invalidate a route, a status meaning or a whole flow, and the API gets
restructured whenever a sounder shape turns up.

So do not spend effort protecting consumers. No deprecation cycles, no
compatibility shims, no keeping a worse design because something might depend on
it. Rename the thing, change the signature, delete the variant. Getting the shape
right now is worth more than any amount of stability, and this is the phase where
that trade is free.

## Commands

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
```

All four must be clean. CI enforces each on Windows and Linux. The docs one
matters more than it looks - broken intra-doc links are the usual way a change
here regresses.

## Editing rules

**Always read files before editing them.** Never assume file contents from memory
or prior context. When making bulk edits, read all target files first.

**Never use em dashes (or en dashes).** Not in prose, doc comments, docs, or
commit messages. Use a plain hyphen with spaces (` - `) or restructure the
sentence so no dash is needed. Numeric ranges use a bare hyphen (`1-5`).

**Never use section signs.** Write `section 1.5`, not `§1.5` - easier for
humans to write and read.

**Avoid trivially descriptive comments.** Only comment non-obvious business logic,
workarounds, edge cases, or "why" decisions. Do not add inline comments that
restate what the code already expresses. Strip narration comments before
committing. Document all public APIs with `///`.

Doc comments here carry things that were **measured against a live client** - a
status code that surprised someone, a route spelling that cost a debugging
session. That knowledge exists nowhere else, so preserve it verbatim when moving
code.

## Writing docs and doc comments

The rules above govern characters. These govern grammar. They apply to prose:
doc comments, `docs/`, commit messages and PR text. They do not apply to code
or to identifiers.

- **Active voice.** Write "the parser reads the file", not "the file is read by
  the parser". Passive voice is correct only when the actor is unknown or does
  not matter.
- **Simple tenses only.** Write "we measured 204", not "we have measured 204".
- **One name for one thing.** Do not rotate check, verify, validate and confirm
  for one action. Pick one word and reuse it.
- **Short common words.** Use "start", not "initiate". Use "use", not
  "utilize". Use "make sure", not "ensure".
- **A verb for an action.** Write "analyze the log", not "perform an analysis
  of the log".
- **No phrasal verbs.** Not "spin up", not "dive into", not "kick off".
- **No marketing adjectives.** Not "seamless", not "robust", not "powerful".
- **Sentence length.** An instruction stops at 20 words. Any other sentence
  stops at 25 words. Split a longer one.
- **No semicolons.** Write two sentences.
- **Noun clusters stop at three words.** Unpack a longer one, or hyphenate it.
- **One topic per paragraph**, and six sentences at most.

Never drop a fact, a number or a scope qualifier to meet a length cap. Keep the
longer sentence. Doc comments here carry measurements, and a grammar rule never
justifies the loss of one.

## Layout

| Module       | Crate | Talks to                                        |
| ------------ | ----- | ----------------------------------------------- |
| `client`     | core  | the remoting server, at the transport level. Owns `Method`/`StatusCode`. reqwest appears nowhere else |
| `endpoint`   | core  | operations as values: the `Endpoint` trait, `EndpointBuilder`, `EndpointMeta` |
| `retry`      | core  | how a request is repeated when it does not stick |
| `route`      | core  | versioned routes as structured data, and the `routes!` macro |
| `lockfile`   | core  | the remoting lockfile on disk                   |
| `processes`  | core  | the Windows process table (lockfile liveness needs it) |
| `types`      | core  | shapes no single namespace owns                 |
| `namespaces` | api   | one module per API namespace, plus `ClientExt`  |
| `models`     | api   | the data the API carries, grouped by namespace  |
| `ids`        | facade | well-known product / patchline identifiers     |
| `installs`   | facade | `RiotClientInstalls.json` on disk              |
| `launch`     | facade | orchestration: pick a namespace and drive it   |
| `session`    | facade | orchestration that outlives a request          |
| `models_ext` | facade | `ProductExt`/`PatchlineExt` - behaviour for generated model types |

## Invariants - do not break these without being asked

- **Dependencies point downward only** - facade → api → core. Cargo refusing a
  cycle is the enforcement. Do not work around it with a re-export or a feature.
- **Nothing hand-written belongs in `ritoclient-api`** beyond what a generator
  would write. It must not loop, sleep, call the OS, name a launcher type, or
  decide what a status means. Its `Cargo.toml` is the allowlist.
- **Every namespace gets a folder** (`namespaces/<ns>/`) with `mod.rs` for the
  handler, `routes.rs` for the routes, and `endpoints.rs` for the endpoint types
  and their `EndpointMeta` table, even at one route.
- **`routes.rs` is one `routes!` invocation.** It declares the constants and the
  namespace's `ALL` table from the same list. Namespace written once, version
  written per route - a namespace serves several versions at a time.
- **Handlers construct an endpoint and pick a finisher, nothing else.** The
  operation itself is an `Endpoint` impl in `endpoints.rs`. Cross-cutting
  behaviour is a combinator on core's `EndpointBuilder`, written once.
- **`models/` mirrors `namespaces/`.** `models::flat` is private generated
  storage. Grouping modules re-export from it. Behaviour goes on the facade's
  extension traits (`models_ext.rs`), never as an inherent `impl`.
- **Handlers return `Option<T>` or `Result<Response, RequestError>`, nothing
  else.** Read-only calls answer `Option`. Only the facade's orchestration
  returns `LauncherError`.
- **A status is data, not an error.** `RequestError` means no round trip
  happened. What a 404 means is the caller's call, because it differs per route.
- **Orchestration lives in the facade.** Anything that polls, spawns a
  thread, or picks *which* route to drive belongs in `launch` or `session`.
- **Handlers are named `<Namespace>Handler`**, to avoid colliding with the
  identically-named model types.

## API design

Two references govern every public item this workspace adds:

- Rust API Guidelines checklist - <https://rust-lang.github.io/api-guidelines/checklist.html>
- Microsoft Rust Guidelines - <https://microsoft.github.io/rust-guidelines/>

Read them when you add or reshape a public type or function. Cite the item that
drives a change, so a reviewer can check the reasoning. Where the two disagree,
say so and record the choice.

Settled decisions, so nobody re-litigates them:

- **A receiver beats a parameter list.** Free functions that share context want
  a type. State the shared inputs on the type once, then make the operations
  inherent methods.
- **Builders take the optional inputs.** Required inputs are arguments to the
  constructor. `build()` validates what nothing earlier can.
- **Accept `impl Into<String>` and `impl AsRef<Path>`** on public inputs. The
  caller decides where a copy happens.
- **Derive the common traits eagerly**: `Debug`, `Clone`, `PartialEq`, `Eq`,
  `Hash`. Add `Display` to any type that a log line formats by hand.
- **Public enums are `#[non_exhaustive]`.** A new variant then costs nothing.
- **A wire string becomes a typed enum at the facade.** The generator carries
  enums as `String`, which tolerates a variant Riot adds. The facade types it
  with a catch-all variant, so a `match` gets checked.
- **Name the catch-all `Other`.** `Unknown` is a value Riot sends. A catch-all
  named `Unknown` makes a real answer look like a parse failure.
- **Write a state predicate as a negative test.** "Has it ended?" tests against
  the one value that means "not yet". A list of the endings reports a future
  variant as live.
- **One `From` beats a repeated `map_err`.** A conversion written at six call
  sites belongs on the error type.
- **Every public item gets a rustdoc example.** Mark it `no_run` when it needs a
  live client.
- **A detached thread returns a handle.** The handle must not cancel on drop
  when the old signature returned nothing. Cancelling on drop turns an existing
  call into a silent no-op.
- **The crate keeps its `prelude`**, against `M-NO-PRELUDE`. The extension
  traits exist only because an inherent `impl` cannot cross a crate, so one
  `use` for all of them is worth the deviation.

Two habits, because both cost this workspace a wrong answer:

- Verify a claim against the code before you assert it. Read the derives, count
  the call sites, run the grep.
- Withdraw a recommendation that does not survive contact with the code, and say
  what changed your mind.

## Hard rules

- Never log the lockfile password. `Lockfile`'s `Debug` redacts it.
- `/product-session/v1/sessions` returns an RSO authorization key inside
  `launchConfiguration.arguments`. Strip it before dumping a session payload.
- Do not add endpoint wrappers for `/rso-auth`, `/rso-authenticator`,
  `/player-account`, `/entitlements` or `/payments`.
- The crates name no products and spawn no game executable. See the `# Scope`
  section of `crates/ritoclient/src/lib.rs` for why the second one is a
  technical fact rather than a policy.

## Testing

Unit tests never touch a live client. Anything that needs one goes in
`examples/`, which CI does not run.
