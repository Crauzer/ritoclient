# How endpoints are organised

Why the crate looks the way it does, what the prior art actually does, and the decision that had to
be taken before the generator in
[`../plans/api-surface-codegen.md`](../plans/api-surface-codegen.md) could be written. The decision
was taken and built - see [the last section](#the-decision-taken).

## The problem

126 namespaces, 1261 functions, ~150 of them in scope. Four namespaces are wrapped by hand today.
Whatever shape endpoints take has to survive being emitted 150 times by a program rather than
written once by a person.

## What other large API crates do

Four patterns, from reading the sources rather than from memory.

### A. Paths built inline - octocrab, async-stripe

`format!("/repos/{owner}/{repo}/issues/{number}")` at the call site. No route constants at all.
Builders are `#[derive(serde::Serialize)]` structs that serialise themselves as the query string.

Simple and readable, and it works because those APIs have **one version across the whole surface**
(octocrab sends it as a header). It gives up any ability to enumerate what the crate covers.

### B. A central route enum - twilight-http

`routing::Route` is one big enum with a `method()` and a `Display` impl that renders the path. Every
request goes through it. It exists because Discord's rate limits are keyed per route, so twilight
*needs* a canonical identity for each one.

Enumerable and uniform. The cost is that one enum with 150+ variants becomes the file everything
touches.

### C. Endpoint-as-a-type - the `gitlab` crate

Each endpoint is a struct implementing an `Endpoint` trait (`method`, `endpoint`, `parameters`).
Behaviour comes from generic combinators applied to any endpoint: `api::paged(…)`,
`api::ignore(…)`, `api::raw(…)`.

This is the most automatable of the four. Pagination, ignoring a body, and returning raw bytes get
written **once** instead of per operation, which matters at 150 endpoints.

### D. Generated operation modules - aws-sdk-rust, progenitor, openapi-generator

One module per operation, everything emitted. Total uniformity; large repos; the generator is the
only thing that understands the layout.

### Versioning, specifically

| Crate         | How the version is expressed                             |
| ------------- | -------------------------------------------------------- |
| octocrab      | An HTTP header, one for the whole API                     |
| twilight-http | One uniform `API_VERSION` const                           |
| azure-sdk     | Per-operation `api-version` query constant                |
| **kube-rs**   | **Per-resource `ApiResource { group, version, kind, plural }`** |

kube-rs is the only real prior art for heterogeneous per-resource versioning, and it agrees with
what this crate needs: a `Route` that carries its own version, because
`rnet-product-registry` serves v1 and v4 simultaneously.

## What this crate took

**octocrab's handler-per-namespace scheme, made uniform.** A handle per namespace obtained from the
client (`client.product_registry()`), with `models/` mirroring `namespaces/` file-for-file.

Two deliberate divergences:

1. **Every namespace gets a folder, and `routes.rs` always exists** - even at one route. octocrab
   splits only when a module grows, which suits ~30 hand-written namespaces. Here the whole crate is
   generator output - `mod.rs`, `routes.rs` and `endpoints.rs` alike, no protected file among them
   (see [`layering.md`](./layering.md)) - and *a layout that changes shape at some size threshold
   cannot be a generator target*.
2. **Routes are structured values, not `format!` strings** - `Route { namespace, version, resource }`,
   closer to kube-rs's `ApiResource` than to octocrab. The version is the part most likely to break
   and a string literal hides it.

Route tables are declared by the `routes!` macro so the constants and the namespace's `ALL` table
come from one list - a route cannot be declared and left out of the table, which is the only way a
hand-written table ever goes wrong. `namespaces::ALL_ROUTES` merges them and `namespaces::routes()`
flattens; that iterator is what a drift check compares against a snapshot.

Handlers are `<Namespace>Handler` rather than `<Namespace>`: the client's namespace names and its
type names overlap heavily, and `ProductSessionHandler` beside
`models::product_session::ProductSession` is the collision the suffix prevents at 126 namespaces.

## The decision, taken

**Endpoint-as-a-type (pattern C), decided and built 2026-07-29.** The reasoning that carried it:

- It is what a generator wants to emit. A struct with a trait impl is mechanical; a method on a
  handler has to be threaded into an existing `impl` block.
- Cross-cutting behaviour gets written once as a combinator rather than 150 times: `send()` for the
  callers that read statuses, `json()`, `ok()` for the read-only-means-`Option` convention,
  `ignore()` for the 204 routes.
- Handlers survive. They stay the ergonomic front door and keep the measured doc comments; they
  just delegate.

The shape as built differs from the `gitlab` crate's in one respect: `METHOD` and `ROUTE` are
associated **consts**, not methods - the verb and path identity are properties of the operation,
readable without constructing one, at the cost of object safety (enumeration goes through the
plain-data `EndpointMeta` tables instead of `dyn Endpoint`). The trait and its `EndpointBuilder`
live in `ritoclient-core/src/endpoint.rs`; every namespace carries an `endpoints.rs` beside its
`routes.rs`; the four hand-written namespaces are converted and stand as the generator's acceptance
fixture, whose contract is in [the codegen plan](../plans/api-surface-codegen.md). The workspace
split that preceded the conversion is documented in [`layering.md`](./layering.md); the
step-by-step plan that drove it was retired once executed in full, 2026-07-29.
