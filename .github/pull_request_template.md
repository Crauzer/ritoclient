## What and why

<!-- If this adds or corrects a route, say how it was confirmed: a probe against
     a live client, swagger, or derivation. An unconfirmed spelling is fine -
     just say so in the route's doc comment. -->

## Checklist

- [ ] `cargo test --workspace --all-features`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo fmt --all`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features`
- [ ] Public items documented; no comments that restate the code
- [ ] Nothing logs a lockfile password or an RSO authorization key
