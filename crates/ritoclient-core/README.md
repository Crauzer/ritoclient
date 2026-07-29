# ritoclient-core

Transport for the Riot Client's local loopback API: the connection, the retry
policy, the route vocabulary, and the lockfile on disk. Mechanism, no policy -
it knows no namespace, models no payload, and launches nothing.

Most consumers want [`ritoclient`](https://crates.io/crates/ritoclient), which
re-exports this crate and adds the typed API surface and the launch
orchestration on top.

Apache-2.0. See [LICENSE-APACHE](LICENSE-APACHE).
