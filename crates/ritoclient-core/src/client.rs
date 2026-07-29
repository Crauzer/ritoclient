//! The connection to the Riot Client's remoting server.
//!
//! [`Client`] is the low-level half of this crate: it can issue any request
//! against any path the client serves, and it is what the typed endpoint modules
//! are built on. Reach for it directly when nothing here models what you need.
//!
//! # Status codes are data, not errors
//!
//! [`RequestError`] means a request did not complete a round trip - nothing was
//! listening, the connection failed, a body would not encode. Every HTTP status
//! the client answers with arrives as a [`Response`], including 404 and 5xx.
//!
//! That is not squeamishness about error types; it is that a status means
//! different things on different routes. A tray-idle client answers 404 for the
//! whole `product-launcher` namespace because the plugin is not registered yet,
//! and that is precisely the state a launch waits out - while the same 404 from
//! a read query means give up. See [`RiotError`] for the field that tells those
//! apart, and decide per call site.

use std::fmt;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::lockfile::live_lockfile;
use crate::retry::RetryPolicy;
use crate::types::RiotError;

/// Read-only queries answer instantly or not at all - they run on paths where a
/// user is waiting (first-run detection), so they fail fast instead.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// The verb of a request.
///
/// This crate's own enum rather than an HTTP library's type, so the library
/// stays a private detail of this module and the generated endpoint layer never
/// freezes its semver into 150 `const METHOD`s. Four variants because the local
/// API uses nothing else; plain `Copy` data because the endpoint metadata tables
/// want to carry it in `const`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    fn as_reqwest(self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Post => reqwest::Method::POST,
            Self::Put => reqwest::Method::PUT,
            Self::Delete => reqwest::Method::DELETE,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        })
    }
}

/// An HTTP status, exactly as the wire carried it.
///
/// **No validation.** Riot answers refusals with codes outside the standard set
/// (464 for an unaccepted ToS), and a type that rejected what the client
/// actually says would be wrong on first contact. Classifying a status is the
/// caller's job; this type only carries it and answers the range questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode(u16);

impl StatusCode {
    pub const NO_CONTENT: StatusCode = StatusCode(204);
    pub const NOT_FOUND: StatusCode = StatusCode(404);

    pub const fn from_u16(code: u16) -> Self {
        Self(code)
    }

    pub const fn as_u16(self) -> u16 {
        self.0
    }

    pub const fn is_success(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    pub const fn is_client_error(self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    pub const fn is_server_error(self) -> bool {
        self.0 >= 500 && self.0 < 600
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a request never produced a response.
///
/// Note what is absent: there is no variant for an unhappy status code. A client
/// that answered, answered.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RequestError {
    /// No live Riot Client - no lockfile, or the pid it names is gone.
    #[error("no Riot Client is running")]
    NoClient,

    /// The request never completed: refused, timed out, TLS failed.
    #[error("{0}")]
    Transport(String),

    /// A request body would not serialize.
    #[error("could not encode the request body: {0}")]
    Encode(String),

    /// A response body was not the shape the caller asked for.
    #[error("unexpected response shape: {0}")]
    Decode(String),
}

/// What the Riot Client answered.
#[derive(Debug, Clone)]
pub struct Response {
    status: StatusCode,
    body: String,
}

impl Response {
    /// Construct a response by hand.
    ///
    /// Public so tests and fakes above this crate can build one; responses
    /// normally come out of [`RequestBuilder::send`].
    pub fn new(status: StatusCode, body: String) -> Self {
        Self { status, body }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn into_body(self) -> String {
        self.body
    }

    /// Deserialize the body.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, RequestError> {
        serde_json::from_str(&self.body).map_err(|e| RequestError::Decode(e.to_string()))
    }

    /// The client's error payload, when the body is one.
    pub fn riot_error(&self) -> Option<RiotError> {
        serde_json::from_str(&self.body).ok()
    }
}

/// Whether the live client serves a path, read off its answer.
///
/// Existence is a property of the client being talked to, not of any table:
/// a tray-idle client serves almost nothing, plugins register as the client
/// boots, and being listed in `/help` does not imply the REST route is mounted
/// (`GetRiotclientappV1IsXbgpRunning` is listed while its route 404s). So the
/// only honest answer is the one the client gives at request time, and its
/// protocol distinguishes three states: anything other than a 404 - a success,
/// a 405 for the wrong verb, even a refusal - proves a handler is mounted, and
/// a 404 splits on [`RiotError::error_code`].
///
/// Obtained from [`Client::probe`], or via `From<&Response>` on an answer
/// already in hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Presence {
    /// A handler answered. The route is live on this client, whatever the
    /// status said.
    Serving,
    /// 404 `RPC_ERROR`: the route is registered but its handler is
    /// unavailable - a plugin still starting, or one not applicable to this
    /// account. The retryable one.
    Registered,
    /// 404 `RESOURCE_NOT_FOUND`: no such route on this client - a wrong
    /// spelling, or a plugin that is not registered at all. Retrying changes
    /// nothing until the client's state does (a tray-idle client waking).
    Absent,
}

impl From<&Response> for Presence {
    fn from(response: &Response) -> Self {
        if response.status() != StatusCode::NOT_FOUND {
            return Self::Serving;
        }
        match response.riot_error() {
            Some(error) if error.is_rpc_error() => Self::Registered,
            _ => Self::Absent,
        }
    }
}

/// A connection to the Riot Client's remoting server.
///
/// Holds one HTTP client and **resolves the lockfile per attempt** rather than
/// storing a port. That is the whole point of the type: waking the client
/// restarts its remoting listener on a *new port* under the same pid, so a
/// cached port is dead within a second of a wake and the pid is not a staleness
/// signal for it. Making that the connection's business rather than each call
/// site's is what stops the next endpoint from getting it wrong.
///
/// Cheap to clone the handle around; cheap to build. Construction only fails if
/// the HTTP stack itself will not initialise.
pub struct Client {
    http: reqwest::blocking::Client,
    retry: RetryPolicy,
    timeout: Duration,
}

/// Configures a [`Client`].
pub struct ClientBuilder {
    retry: RetryPolicy,
    timeout: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            retry: RetryPolicy::none(),
            timeout: QUERY_TIMEOUT,
        }
    }
}

impl ClientBuilder {
    /// The policy every request inherits unless it sets its own.
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
        self
    }

    /// The timeout every request inherits unless it sets its own.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the client.
    ///
    /// It accepts Riot's self-signed remoting certificate, which is only
    /// acceptable because this is a loopback connection to a port and password
    /// read from a file only the current user can read. Extracting Riot's root
    /// certificate instead would break every time they rotate it.
    ///
    /// The cert-accepting `reqwest::Client` is a private field and is never
    /// handed out, and every URL is built from a lockfile this type read itself,
    /// so nothing it can address lives off loopback.
    pub fn build(self) -> Result<Client, RequestError> {
        let http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| {
                RequestError::Transport(format!("could not build the HTTP client: {e}"))
            })?;

        Ok(Client {
            http,
            retry: self.retry,
            timeout: self.timeout,
        })
    }
}

impl Client {
    pub fn new() -> Result<Self, RequestError> {
        Self::builder().build()
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub fn request(&self, method: Method, path: impl Into<String>) -> RequestBuilder<'_> {
        RequestBuilder {
            client: self,
            method,
            path: path.into(),
            body: Ok(None),
            timeout: None,
            retry: None,
        }
    }

    pub fn get(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::Get, path)
    }

    pub fn post(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::Post, path)
    }

    pub fn put(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::Put, path)
    }

    pub fn delete(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::Delete, path)
    }

    /// `GET <path>`, deserialized, with every failure collapsed to `None`.
    ///
    /// Best-effort by design: no client, a refused connection, a 404 because the
    /// plugin isn't registered, a body whose shape moved - all `None`. Callers
    /// use this to *enrich* what they already know, so "the client couldn't tell
    /// us" must never become an error the user sees.
    pub fn get_json<T: DeserializeOwned>(&self, path: impl Into<String>) -> Option<T> {
        let path = path.into();
        let response = self
            .get(&path)
            .send()
            .inspect_err(|e| tracing::debug!("GET {path} failed: {e}"))
            .ok()?;

        if !response.is_success() {
            tracing::debug!("GET {path} answered HTTP {}", response.status());
            return None;
        }

        response
            .json()
            .inspect_err(|e| tracing::warn!("GET {path} returned an unexpected shape: {e}"))
            .ok()
    }

    /// Ask the live client whether it serves `path`, without invoking anything
    /// mutating.
    ///
    /// The probe is a plain `GET`: a POST-only route answers 405, which is
    /// [`Presence::Serving`] - existence confirmed without firing the call.
    /// Takes anything path-shaped - a [`Route`](crate::Route), a bound
    /// [`Endpoint::path`](crate::Endpoint::path), a raw string for paths
    /// outside the route convention. A parameterized path must be bound first;
    /// a literal `{placeholder}` just answers as no such route. And on a bound
    /// path, a 404 can be about the entity rather than the route - the
    /// `errorCode` split is the best signal the client gives.
    ///
    /// `Err` still means no round trip happened, per this crate's rule; the
    /// three-way [`Presence`] is only ever read off an actual answer.
    pub fn probe(&self, path: impl Into<String>) -> Result<Presence, RequestError> {
        let response = self.get(path).send()?;
        Ok(Presence::from(&response))
    }

    /// One attempt. Re-reads the lockfile, so a port that moved since the last
    /// attempt is picked up rather than retried against.
    fn attempt(
        &self,
        method: Method,
        path: &str,
        body: Option<&str>,
        timeout: Duration,
    ) -> Result<Response, RequestError> {
        let lockfile = live_lockfile().ok_or(RequestError::NoClient)?;

        let mut request = self
            .http
            .request(
                method.as_reqwest(),
                format!("{}{path}", lockfile.base_url()),
            )
            .basic_auth("riot", Some(&lockfile.password))
            .timeout(timeout);

        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }

        let response = request
            .send()
            .map_err(|e| RequestError::Transport(describe(&e)))?;

        let status = StatusCode::from_u16(response.status().as_u16());
        let body = response
            .text()
            .map_err(|e| RequestError::Transport(describe(&e)))?;

        Ok(Response::new(status, body))
    }
}

/// A request under construction.
pub struct RequestBuilder<'a> {
    client: &'a Client,
    method: Method,
    path: String,
    /// Deferred so `json()` can stay chainable; surfaced by [`Self::send`].
    body: Result<Option<String>, RequestError>,
    timeout: Option<Duration>,
    retry: Option<RetryPolicy>,
}

impl RequestBuilder<'_> {
    /// Send `value` as the JSON body.
    ///
    /// Note the client's argument convention: a function taking a single
    /// non-path argument wants the request body to **be** that value, not an
    /// object wrapping it. `POST /riotclientapp/v1/new-args` wants a bare
    /// `["--flag"]`; `{"args": [...]}` is rejected as not a collection.
    pub fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.body = serde_json::to_string(value)
            .map(Some)
            .map_err(|e| RequestError::Encode(e.to_string()));
        self
    }

    /// Send a body verbatim.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Ok(Some(body.into()));
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    /// Issue the request, retrying per the policy in force.
    pub fn send(self) -> Result<Response, RequestError> {
        let body = self.body?;
        let timeout = self.timeout.unwrap_or(self.client.timeout);
        let policy = self.retry.as_ref().unwrap_or(&self.client.retry);

        let started = Instant::now();
        let mut attempt = 1;

        loop {
            let outcome = self
                .client
                .attempt(self.method, &self.path, body.as_deref(), timeout);

            let Some(delay) = policy.next_delay(&outcome, attempt, started.elapsed()) else {
                return outcome;
            };

            tracing::debug!(
                "{} {} attempt {attempt} did not stick ({}); retrying in {delay:?}",
                self.method,
                self.path,
                match &outcome {
                    Ok(response) => format!("HTTP {}", response.status()),
                    Err(e) => e.to_string(),
                }
            );

            std::thread::sleep(delay);
            attempt += 1;
        }
    }

    /// Issue the request and deserialize a successful body.
    ///
    /// A non-success status is [`RequestError::Decode`] rather than a distinct
    /// variant, because this helper is for routes whose failure the caller has
    /// already decided not to inspect. Anything else should call [`Self::send`].
    pub fn send_json<T: DeserializeOwned>(self) -> Result<T, RequestError> {
        let response = self.send()?;
        if !response.is_success() {
            return Err(RequestError::Decode(format!(
                "HTTP {}: {}",
                response.status(),
                response.body().trim()
            )));
        }
        response.json()
    }
}

/// Render a request failure with its cause, and name the two that matter.
///
/// `reqwest::Error`'s own `Display` stops at *"error sending request for url
/// (...)"* and never walks its source, so a 20-second timeout and an instantly
/// refused connection print the same line. Those two say opposite things about
/// what the client is doing - one is busy, the other is gone - and telling them
/// apart in a bug report is the whole point of this function.
fn describe(error: &reqwest::Error) -> String {
    let mut out = if error.is_timeout() {
        "the client accepted the connection but did not answer in time".to_string()
    } else if error.is_connect() {
        "nothing was listening on the port the lockfile named".to_string()
    } else {
        error.to_string()
    };

    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        out.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from a live refusal, non-standard status and all.
    const EULA_REFUSAL: &str = r#"{"errorCode":"eula_not_accepted","httpStatus":464,"implementationDetails":{},"message":"eula_not_accepted: Cannot run product 'league_of_legends' patchline 'live' because the player didn't accept EULA Terms of Service"}"#;

    fn response(status: u16, body: &str) -> Response {
        Response::new(StatusCode::from_u16(status), body.to_string())
    }

    #[test]
    fn a_refusal_parses_into_its_code_and_prose() {
        let error = response(464, EULA_REFUSAL).riot_error().unwrap();

        assert_eq!(error.error_code, "eula_not_accepted");
        assert_eq!(error.http_status, Some(464));
        assert!(
            error.prose().starts_with("Cannot run product"),
            "the code is a field, so the prose must not repeat it: {}",
            error.prose()
        );
    }

    /// Same status, opposite meanings - the distinction the harness refuses to
    /// make on the caller's behalf, so it has to be legible on the payload.
    #[test]
    fn the_two_flavours_of_404_are_told_apart() {
        let absent = response(404, r#"{"errorCode":"RESOURCE_NOT_FOUND","message":"no"}"#)
            .riot_error()
            .unwrap();
        assert!(absent.is_resource_not_found());
        assert!(!absent.is_rpc_error());

        let unavailable = response(404, r#"{"errorCode":"RPC_ERROR","message":"nope"}"#)
            .riot_error()
            .unwrap();
        assert!(unavailable.is_rpc_error());
        assert!(!unavailable.is_resource_not_found());
    }

    /// The classification behind [`Client::probe`]: anything but a 404 proves
    /// a mounted handler, and the two 404 flavours split on the errorCode.
    #[test]
    fn presence_reads_the_clients_three_answers() {
        assert_eq!(Presence::from(&response(200, "true")), Presence::Serving);
        assert_eq!(Presence::from(&response(405, "")), Presence::Serving);
        // A refusal is an answer: the route that refused is live.
        assert_eq!(
            Presence::from(&response(464, EULA_REFUSAL)),
            Presence::Serving
        );
        assert_eq!(
            Presence::from(&response(
                404,
                r#"{"errorCode":"RPC_ERROR","message":"nope"}"#
            )),
            Presence::Registered
        );
        assert_eq!(
            Presence::from(&response(
                404,
                r#"{"errorCode":"RESOURCE_NOT_FOUND","message":"no"}"#
            )),
            Presence::Absent
        );
        // A 404 whose body is not the client's error payload still reads
        // absent rather than inventing a third guess.
        assert_eq!(Presence::from(&response(404, "")), Presence::Absent);
    }

    #[test]
    fn a_body_that_is_not_an_error_payload_yields_no_riot_error() {
        assert!(response(200, "true").riot_error().is_none());
        assert!(response(409, "patchline is locked").riot_error().is_none());
    }

    /// A refusal we cannot parse still has to reach the user, so the raw body
    /// stays available rather than being consumed by a failed parse.
    #[test]
    fn an_unparseable_body_is_still_readable() {
        let response = response(409, "patchline is locked");
        assert!(response.riot_error().is_none());
        assert_eq!(response.body(), "patchline is locked");
        assert!(!response.is_success());
    }

    #[test]
    fn a_status_is_not_an_error() {
        for status in [200, 204, 404, 464, 500] {
            let response = response(status, "{}");
            assert_eq!(response.status().as_u16(), status);
        }
    }

    #[test]
    fn json_decodes_the_body() {
        assert!(response(200, "true").json::<bool>().unwrap());
        assert!(response(200, "not json").json::<bool>().is_err());
    }

    /// The client wants a bare value, not an object wrapping it - sending
    /// `{"args": [...]}` to `new-args` is rejected outright.
    #[test]
    fn json_encodes_a_bare_value() {
        let client = Client::new().unwrap();
        let request = client
            .post("/riotclientapp/v1/new-args")
            .json(&["--launch-product=league_of_legends"]);

        assert_eq!(
            request.body.unwrap().as_deref(),
            Some(r#"["--launch-product=league_of_legends"]"#)
        );
    }

    /// The lockfile is read per attempt, so with no client running every call
    /// answers `NoClient` rather than reusing anything cached at build time.
    #[test]
    fn requests_without_a_live_client_report_no_client() {
        let client = Client::new().unwrap();

        if live_lockfile().is_some() {
            return; // a real client is up on this machine; nothing to assert
        }
        assert!(matches!(
            client.get("/patch/v1/installs").send(),
            Err(RequestError::NoClient)
        ));
        assert!(
            client
                .get_json::<serde_json::Value>("/patch/v1/installs")
                .is_none()
        );
        assert!(matches!(
            client.probe("/patch/v1/installs"),
            Err(RequestError::NoClient)
        ));
    }
}
