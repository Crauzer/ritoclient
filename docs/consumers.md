# Consumers, and what depending on this crate currently costs

## How ltk-manager depends on it today

Pinned by git rev, in that repo's `[workspace.dependencies]` so its two dependents cannot drift onto
different revs:

```toml
ritoclient-api = { git = "https://github.com/Crauzer/ritoclient", rev = "<sha>" }
```

(That was the crate's old name; since the three-crate split, the one to depend on is `ritoclient`,
and moving to it is the same one-line import edit the rename was always going to cost.) It will
stay a rev pin until the crate is published. Publishing needs a `CARGO_REGISTRY_TOKEN` repository
secret; `.github/workflows/release.yml` does the rest on a `v*` tag.

**Publishing is worth delaying** until the remaining open decisions in
[`plans/api-surface-codegen.md`](plans/api-surface-codegen.md) are taken - the endpoint shape is
settled and built, but a published API is one that has to be broken rather than changed, and the
generator has not yet exercised it at full width.

## The `ts` feature is broken for external consumers

The `ts` feature derives `ts_rs::TS` on the six types that cross an IPC boundary - `LauncherError`,
`LaunchOutcome`, `LaunchProgress`, `LaunchRoute`, `LaunchStage`, `LaunchTarget`.

`ts_rs` exports by generating a `#[test]` that writes the `.ts` file. **Cargo never compiles or runs
a dependency's tests.** So enabling this feature from a downstream crate produces no bindings at all.

This is not theoretical. It was verified by deleting `LaunchTarget.ts` in ltk-manager and confirming
a full `cargo test --workspace` did not recreate it. Those six binding files are now hand-maintained
downstream: if one of these types changes here, the `.ts` goes stale **silently** - no build error,
just a frontend type that quietly disagrees with the backend.

**Recommended fix: drop the feature.** A general-purpose Apache-2.0 client carrying a `ts-rs`
dependency to serve one consumer's IPC layer is the same category of leak that the `LaunchObserver`
trait exists to prevent. The downstream crate should own its own IPC-facing shapes. This is cheap
before publishing and breaking afterwards.

Until then, the feature is documented honestly in the crate README and its export artefacts are kept
out of the package by `exclude = ["bindings/"]` in `Cargo.toml` - a `.gitignore` does not reliably
gate `cargo package`, which the CI `publish --dry-run` job caught on its first run.

## API notes that downstream got wrong once

- **`LaunchTarget` deliberately has no `Default`.** Which product to launch is not this crate's to
  assume, and a default of two empty strings is a launch target naming no product. ltk-manager
  supplies `league_target()` instead. The one call site that missed this was inside a
  `#[cfg(not(target_os = "windows"))]` test, so it only failed on Linux CI - worth remembering that
  **this crate's platform gating hides breakage from a Windows-only test run.**
