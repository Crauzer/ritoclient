# ritoclient-api

Typed namespaces and models for the Riot Client's local API, built on
[`ritoclient-core`](https://crates.io/crates/ritoclient-core).

This crate is the generator's target: everything in it is (or is slated to
become) generator output, and its dependency list is the allowlist that keeps
launcher policy out of generated code. Hand-written orchestration lives in
[`ritoclient`](https://crates.io/crates/ritoclient), which is the crate most
consumers want.

Apache-2.0. See [LICENSE-APACHE](LICENSE-APACHE).
