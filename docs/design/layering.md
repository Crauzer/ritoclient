# Layers, and which way they point

The crate was built top-down: a launcher was needed, and the API surface grew underneath it as far
as that launcher required. That order put launcher knowledge inside the layer a generator is
supposed to own. This is the correction.

## The evidence

The namespace layer imports from the layer above it, and the transport imports from the layer below
it. Both directions are wrong:

| Leak                                        | Where                                                 |
| ------------------------------------------- | ----------------------------------------------------- |
| `use crate::launch::LaunchTarget`           | `product_launcher/routes.rs`, `app_args/mod.rs`       |
| `use crate::window::allow_foreground`       | `app_args/mod.rs`                                     |
| `LauncherError` as a return type            | `lifecycle/mod.rs`, `product_launcher/mod.rs`         |
| `LaunchAttempt`, and its `NotReady` reading | `product_launcher/mod.rs`                             |
| A hand-rolled retry loop with a `sleep`     | `app_args/mod.rs`                                     |
| `pub(crate) fn post`                        | `lifecycle/mod.rs`, for `session`'s re-assert loop     |
| `LAUNCH_TIMEOUT`, `WAKE_TIMEOUT`            | `client.rs` - launcher policy sitting in the transport |
| `use crate::namespaces::…Handler` ×4        | `client.rs` - the transport importing the generated layer |

The first six rows are all files a generator would have to write and a human would have to defend.
At four namespaces they are invisible. At 126 they are 126 merge conflicts.

The last two are the ones a grep for launcher symbols does not find. `LAUNCH_TIMEOUT` and
`WAKE_TIMEOUT` are launcher policy in a module that is supposed to be pure mechanism - `QUERY_TIMEOUT`
beside them is fine, it is the transport's own default, but a *launch* and a *wake* are not things
the transport knows about.

The handler imports are worse, and they are the reason this document ends in a crate split rather
than a feature flag. `client.rs` reaches **down-stack into generated code** so that
`client.product_launcher()` can be an inherent method. A `launcher` feature would never flag it:
turn the launcher off and those four imports are still there. A crate boundary flags it on the first
build, because it is a dependency cycle.

`app_args::wake_with_launch_args` is the clearest case. It is not an endpoint wrapper that grew a
few extras; it is **the wake step of the launch wearing a handler's clothes**. It knows the argv
spelling `--launch-product=`, it re-asserts a Win32 foreground grant, it sleeps between attempts,
and it reports a `LauncherError`. None of that is derivable from the client's schema, and none of it
belongs in a file named after a namespace.

## The model

Three layers, three crates, and the generated one in the middle:

```text
crates/ritoclient/        launch, session, progress, installs, window, spawn
                          plus ids, error, and the model extension traits
                          hand-written: ours. Allowed to loop, sleep, and call Win32
                          also the facade - re-exports the two crates below; examples/ lives here
        ↓
crates/ritoclient-api/    namespaces/  handlers, endpoints, routes      gen
                          models/      the types those carry            gen
                          every file in it is generator output
        ↓
crates/ritoclient-core/   client, retry, route, lockfile, processes, types, endpoint
                          owns the wire vocabulary - Method and StatusCode are its types
                          hand-written: transport and repetition, mechanism, no policy

crates/xtask/             snapshot and generator. Unpublished
```

Arrows are `[dependencies]` entries. Cargo will not let them point the other way, which is the whole
mechanism.

`gen` is a label, not a directory. Should it ever want to be a real module, note that **`gen` is a
reserved keyword in edition 2024** and would have to be spelled `r#gen`; `codegen` is the fallback,
matching the `cargo xtask ritoclient-codegen` that writes it.

### Why the generated code gets a crate of its own

Three things a folder inside a crate cannot give it:

- **Regeneration is a wipe, not a merge.** `xtask` deletes `crates/ritoclient-api/src/` and writes it
  from scratch. There is no allowlist of files it may touch, no protected `mod.rs`, no subtree diff -
  the unit of generation is the unit of publication. "Nothing here is hand-written" stops being a
  rule and becomes a description of how the file got there.
- **The dependency list is the allowlist.** `ritoclient-api/Cargo.toml` names `ritoclient-core`,
  `serde` and `serde_json`. Generated code therefore *cannot* reach `windows-sys`, or a launcher
  type, or a sleep - not because a reviewer would object but because the name does not resolve. A
  feature flag only made such a reference fail under one particular CI invocation.
- **The cycle error is unconditional.** It does not depend on CI remembering to run
  `--no-default-features`, and it does not depend on which features a contributor has enabled
  locally.

### The wire vocabulary is ours, so the HTTP client is swappable

`client.rs` currently does `pub use reqwest::{Method, StatusCode}`. That was free at four
namespaces and stops being free the moment a generator writes `const METHOD: Method` 150 times -
reqwest's semver would be frozen into every generated endpoint and into `ritoclient-core`'s public
API with it.

So core owns both types. `Method` is a four-variant `Copy` enum - the local API uses nothing else,
and plain data is what the `EndpointMeta` table wants (`reqwest::Method` is not `Copy`).
`StatusCode` is a thin wrapper over the wire's `u16` carrying the predicates the crate already
leans on (`is_success`, `is_server_error`, `as_u16`, the named constants) and **no validation** -
Riot answers codes outside the standard set (464 for an unaccepted EULA), and a type that rejected
what the wire says would be wrong on first contact.

reqwest then appears in no public signature. It is a private detail of `client.rs`, converted to
and from at the one place a request is actually issued - so swapping the HTTP client later touches
`attempt()` and `describe()`, not the trait, not the generated crate, not a caller.

### What the split costs, and what pays for it

Accepted, not argued with - recording them so nobody rediscovers them as surprises:

| Cost                                                                       | Resolution                                        |
| -------------------------------------------------------------------------- | ------------------------------------------------- |
| `client.product_launcher()` is an inherent `impl Client`, and **an inherent impl must live in the crate defining the type** (E0116) | Becomes a generated `ClientExt` extension trait in `ritoclient-api`, re-exported from `ritoclient::prelude`. The call spelling does not change |
| `Response::new` is `pub(crate)`; `product_launcher`'s tests use it and now live in another crate | Goes `pub` on `ritoclient-core`. Defensible on its own - anyone writing a fake against this crate wants it - and better than a `test-util` feature CI has to remember |
| Upward intra-doc links break. A lower crate cannot link to a higher one, and `RUSTDOCFLAGS="-D warnings"` makes that a build failure | Found at split time, not later. `lifecycle`'s pointer to `hide_for_play_session` becomes prose, or moves to the facade's module docs where the link resolves |
| The model `impl` blocks (`Patchline::is_installed`, `secondary_dir`) are hand-written, live in the crate the wipe erases, and E0116 forbids re-homing an inherent impl | Become extension traits in the facade - `PatchlineExt`, `ProductExt` - re-exported from `ritoclient::prelude` beside `ClientExt`. Their recorded fixtures and tests move with them |
| `lockfile::live_lockfile` proves liveness through the process table - "no client" *means* "the pid is not `RiotClientServices.exe` any more" - so `processes` cannot sit above the transport | `processes` lives in core, carrying `windows-sys`'s ToolHelp half unconditionally. The facade's `launcher` feature gates only the launcher's own Win32 (`AllowSetForegroundWindow`) |
| Three versions published in lockstep                                       | `version.workspace = true`, `=` pins between them, release order core → api → facade |

The `ClientExt` row is the one that changes what a caller types. It is one `use` line, it is the
standard Rust answer (`itertools::Itertools`), and the prelude absorbs it - and it buys the property
that the transport crate no longer knows the generated layer exists.

### Splitting further

`ritoclient` is the facade *and* the launcher. Splitting those apart - a fourth crate,
`ritoclient-launcher`, with `ritoclient` reduced to `pub use` - is mechanical and can happen any
time. The trigger is the launcher wanting a release cadence of its own, or a second hand-written
layer appearing beside it. Until then a `feature = "launcher"` on the facade, default on with
`windows-sys` optional under it, gives a consumer the API without the Win32 dependency, which was the
only concrete reason to want the fourth crate.

### `ritoclient-api` is generated. All of it.

Not "`routes.rs` is generated and `mod.rs` is the file the generator must never touch" - that seam
has to be explained every time someone opens the folder, and a seam that needs explaining is one
that gets crossed. The invariant is one sentence with no exceptions:

> **Nothing in `ritoclient-api` is hand-written. If you want to write something by hand, it does not
> belong there.**

That answers "where does this go?" without anyone having to reason about layers, and it is
mechanically enforced twice over: the crate's `[dependencies]` do not contain the things
hand-written code would reach for, and CI regenerates and fails on `git diff --exit-code`. The four
questions that used to define the boundary - does it loop, does it sleep, does it call the OS, does
it decide what a status *means* - survive as a description of what tends to go wrong, not as a rule
anybody has to apply.

`namespaces/mod.rs` is generated too, including `ALL_ROUTES`, `ALL_ENDPOINTS`, the iterators, and
the `ClientExt` trait carrying the `client.product_launcher()` accessors. `Cargo.toml` is the one
hand-written file in the crate, and it is the allowlist rather than an exception to it.

### The vocabulary test

The one thing still worth checking by eye, because it catches drift before it becomes a leak: the
generated layer speaks the **client's** vocabulary - `productId`, `patchlineId`, `RequestError`,
`Response`. `LaunchTarget`, `LauncherError` and `LaunchAttempt` are the **launcher's** vocabulary.
A generated file that mentions one of ours is a sign the snapshot is being bent to fit a caller.

## What the two problem cases become

The wake keeps its loop. It just keeps it in a file that is allowed to have one - and its only
caller is right there:

```rust
// launch.rs - knows about launching; the endpoint below it does not.
fn wake(client: &Client, target: &LaunchTarget) -> Result<(), LauncherError> {
    let args = [
        format!("--launch-product={}", target.product_id),
        format!("--launch-patchline={}", target.patchline_id),
    ];

    let mut last_reason = String::from("no attempt was made");
    for attempt in 1..=ATTEMPTS {
        // Consumed when a window takes the foreground, so it is re-asserted per attempt.
        allow_foreground();

        match client
            .endpoint(&riotclientapp::NewArgs { args: &args })
            .timeout(WAKE_TIMEOUT)
            .send()
        { /* …unchanged… */ }
    }

    Err(LauncherError::RiotClientUnreachable { reason: last_reason })
}
```

Nothing about the loop got harder; it stopped being a special case, because the file it now lives in
has no rule against loops. `riotclientapp::NewArgs` is generated, knows only its verb, path and
body, and is reusable by anything.

`lifecycle::post` disappears outright. Its only reason to exist was giving `session` a hide that
does not log, and once the endpoint is a value that is `client.endpoint(&lifecycle::Hide).ignore()`
at the call site. Logging is the caller's editorial choice, not a second shape the namespace has to
publish.

`LaunchAttempt` and `refusal()` move to `launch.rs` beside the wait they inform.
`ProductLauncherHandler::launch` returns `Result<Response, RequestError>` like every other generated
handler method, which is the crate's own "a status is data" rule finally applying to the launcher
too.

A note on the loop, for later: it exists because `RetryPolicy` has no "before each attempt" hook.
Adding one would collapse it into configuration, and the layering would hold - the hook is a closure
supplied from `launch.rs`, so `retry.rs` never learns what Win32 is. Worth doing **only if a second
call site wants it.** One explicit loop in a file allowed to have loops is not a problem to solve.

## What full generation costs

Two things, both real, neither fatal.

**The measured doc comments move to `schema/overrides.toml`.** They cannot live in a regenerated
file, and they are the crate's most protected asset - the 404 that means "wait", the spelling that
cost a debugging session, the paragraph explaining why `/quit/switch-background-mode` is not
wrapped. So `overrides.toml` stops being a patch list and becomes **the crate's knowledge base**:
the snapshot is what the client says, the overrides are what we learned, and the generated code is
both, compiled. Prose in TOML is worse to write than prose in Rust; a file the generator must not
touch inside a folder it otherwise owns is worse than that.

**Hand-written conveniences lose their home.** `ProductRegistryHandler::product(id)` filters
`products()` client-side - not an endpoint, so nothing generates it. With the generated code in its
own crate this is not merely awkward, it is E0116: an inherent `impl` for a `ritoclient-api` type
cannot be written in `ritoclient`. The options are an extension trait in the facade, or letting the
one call site write the `.find()` itself. Prefer the second until there are enough of them to argue
about; one one-liner does not justify a trait. When a real cluster appears, it joins the model
extension traits in the facade - **not** `overrides.toml`. The line is:

> **`overrides.toml` carries data, never code** - doc prose, serde aliases, confirmed spellings,
> probe notes. Anything with a function body is hand-written, and hand-written code lives in a
> hand-written crate.

The moment an override entry contains Rust, it is a source file wearing TOML quoting - invisible to
rustfmt, clippy and rust-analyzer alike, which are exactly the tools this workspace holds itself
to.

## Keeping it that way

Two checks, and neither is a convention anyone has to remember:

- **Cargo refuses the cycle.** `ritoclient-core` cannot name `ritoclient-api`, and neither can name
  `ritoclient`. Every row of the leak table at the top of this document is now a build failure.
- **`git diff --exit-code` after regeneration**, in CI. `xtask` wipes and rewrites
  `ritoclient-api/src/`, so a hand edit shows up as a diff on the next run.

The first is what makes the second sufficient: because the generator owns a whole crate rather than
a folder inside one, "regenerate and diff" needs no allowlist of files it is permitted to touch.
