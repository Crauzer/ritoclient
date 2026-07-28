# Security

## Reporting a vulnerability

Please report privately through
[GitHub's security advisory form](https://github.com/Crauzer/ritoclient/security/advisories/new)
rather than opening a public issue.

## What this crate handles

It talks to a loopback HTTPS server already running on the user's own machine,
authenticating with a lockfile only that user can read. It grants no access
anyone running it did not already have. Two things it touches are genuinely
sensitive:

- **The lockfile password.** `Lockfile`'s `Debug` implementation redacts it. Any
  change that would print it - a new `Debug` derive, a log line, an error message
  carrying the raw file contents - is a bug worth reporting.
- **RSO authorization keys.** `/product-session/v1/sessions` returns one inside
  `launchConfiguration.arguments`. Anything that dumps a session payload must
  strip that field first.

The endpoint modules deliberately do not wrap `/rso-auth`, `/rso-authenticator`,
`/player-account`, `/entitlements` or `/payments`. That is a decision about what
this crate has business modelling - it is *not* an access control boundary, and
should not be relied on as one. The low-level `Client` can address any path the
server serves, by design.

## Out of scope

- That the local API is reachable at all. That is how the Riot Client works.
- Anything requiring an attacker to already have the user's account credentials
  or read access to their user profile - at which point the lockfile is the least
  of it.
