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

use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use reqwest::{Method, StatusCode};

use crate::lockfile::live_lockfile;
use crate::namespaces::app_args::AppArgsHandler;
use crate::namespaces::lifecycle::LifecycleHandler;
use crate::namespaces::product_launcher::ProductLauncherHandler;
use crate::namespaces::product_registry::ProductRegistryHandler;
use crate::retry::RetryPolicy;
use crate::types::RiotError;

/// A launch has to survive a client that is mid-startup, so it waits.
///
/// Generous because the client runs its own gates before it spawns anything -
/// a patch-state refresh, a player-affinity token, an up-to-date check - and
/// each is a round trip to Riot's servers. Five seconds covered none of that on
/// a slow link, and a timeout here does **not** cancel the work: the client went
/// on to launch minutes later while we had already reported a failure.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);

/// Waking a tray-idle client is a cheap 204, so it does not get the launch
/// budget - it is retried, and three attempts of [`LAUNCH_TIMEOUT`] would spend
/// a minute before the wait for the client even starts.
pub const WAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Read-only queries answer instantly or not at all - they run on paths where a
/// user is waiting (first-run detection), so they fail fast instead.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

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
    pub(crate) fn new(status: StatusCode, body: String) -> Self {
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

    /// `/product-launcher/v1` - starting a product.
    pub fn product_launcher(&self) -> ProductLauncherHandler<'_> {
        ProductLauncherHandler::new(self)
    }

    /// `/rnet-product-registry/v4` - what is installed, and where.
    pub fn product_registry(&self) -> ProductRegistryHandler<'_> {
        ProductRegistryHandler::new(self)
    }

    /// `/riot-client-lifecycle/v1` - the client's own window.
    pub fn lifecycle(&self) -> LifecycleHandler<'_> {
        LifecycleHandler::new(self)
    }

    /// `/riotclientapp/v1` - the argv handoff.
    pub fn app_args(&self) -> AppArgsHandler<'_> {
        AppArgsHandler::new(self)
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
        self.request(Method::GET, path)
    }

    pub fn post(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::POST, path)
    }

    pub fn put(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::PUT, path)
    }

    pub fn delete(&self, path: impl Into<String>) -> RequestBuilder<'_> {
        self.request(Method::DELETE, path)
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

    /// One attempt. Re-reads the lockfile, so a port that moved since the last
    /// attempt is picked up rather than retried against.
    fn attempt(
        &self,
        method: &Method,
        path: &str,
        body: Option<&str>,
        timeout: Duration,
    ) -> Result<Response, RequestError> {
        let lockfile = live_lockfile().ok_or(RequestError::NoClient)?;

        let mut request = self
            .http
            .request(method.clone(), format!("{}{path}", lockfile.base_url()))
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

        let status = response.status();
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
                .attempt(&self.method, &self.path, body.as_deref(), timeout);

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
        Response::new(StatusCode::from_u16(status).unwrap(), body.to_string())
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
    }
}
