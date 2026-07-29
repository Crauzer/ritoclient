# Docs

Background that does not belong in a doc comment. Anything measured about a *specific route* lives
on that route in the source; these are the documents that span routes.

| Document | What it is |
| --- | --- |
| [`riot-client-local-api.md`](./riot-client-local-api.md) | **The survey.** 1261 functions across 126 namespaces, probed against a live client. The crate's source data - read section 6 before adding a namespace |
| [`launch-protocol.md`](./launch-protocol.md) | How launching actually works, and the approaches tried and rejected |
| [`consumers.md`](./consumers.md) | Where this crate came from, how downstream depends on it, and what the `ts` feature currently costs |
| [`design/endpoint-layout.md`](./design/endpoint-layout.md) | Why the crate is laid out this way, what other API crates do, and the endpoint-shape decision (taken: endpoint-as-a-type) |
| [`design/layering.md`](./design/layering.md) | The three crates and which way they point - why launcher policy does not live in generated code |
| [`plans/api-surface-codegen.md`](./plans/api-surface-codegen.md) | **The working plan.** Steps 2-5: snapshot, codegen, re-base, preamble |

## Start here

Picking the work back up: [`plans/api-surface-codegen.md`](./plans/api-surface-codegen.md) has the
status table and the open decisions.

Adding a namespace by hand: `CONTRIBUTING.md` at the repo root has the conventions;
[`riot-client-local-api.md`](./riot-client-local-api.md) has the measurements.

## Two things to know before changing anything

- **Doc comments in this crate are a record of what was measured against a live client** - a status
  code that surprised someone, a spelling that cost a debugging session. That knowledge exists
  nowhere else. Preserve it verbatim when moving code; do not rewrite it in passing.
- **Windows-only code paths hide breakage.** `cargo test` on Windows does not compile
  `#[cfg(not(target_os = "windows"))]` blocks, and vice versa. CI runs both for this reason.
