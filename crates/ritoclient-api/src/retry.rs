//! Retry and backoff policy for requests to the Riot Client.
//!
//! Retrying is **opt-in**. The default policy is a single attempt, because the
//! calls that dominate this crate are read-only queries on paths where a user is
//! waiting, and those are documented to fail fast rather than wait (see
//! [`crate::client::QUERY_TIMEOUT`]). A policy is set once on a
//! [`Client`](crate::client::Client) or per request, whichever fits.
//!
//! The retry that matters here is transport-level. Waking the client restarts
//! its remoting listener on a new port under the same pid, so a request issued
//! across that window gets a refused connection from a client that is perfectly
//! healthy. Since every attempt re-reads the lockfile, retrying is what turns
//! that into a hiccup instead of a failure.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::client::{RequestError, Response};

/// How long to wait between attempts.
#[derive(Debug, Clone, Copy)]
pub enum Backoff {
    Fixed(Duration),
    Exponential {
        initial: Duration,
        multiplier: f64,
        max: Duration,
    },
}

impl Backoff {
    /// The delay following `attempt` (1-based).
    ///
    /// No jitter: jitter exists to spread a thundering herd across many clients,
    /// and this is one process talking to a loopback server that only it uses.
    fn delay_after(&self, attempt: u32) -> Duration {
        match *self {
            Self::Fixed(delay) => delay,
            Self::Exponential {
                initial,
                multiplier,
                max,
            } => {
                let scaled =
                    initial.as_secs_f64() * multiplier.powi(attempt.saturating_sub(1) as i32);
                Duration::from_secs_f64(scaled).min(max)
            }
        }
    }
}

/// Decides whether an outcome is worth another attempt.
pub type RetryPredicate = Arc<dyn Fn(&Result<Response, RequestError>) -> bool + Send + Sync>;

/// Whether, and how often, a request is retried.
///
/// Bounded by attempts, by a deadline, or by both - whichever runs out first.
/// A policy with neither retries as long as its predicate says to, which is a
/// loop, so [`Self::attempts`] or [`Self::deadline`] is the entry point rather
/// than a bare struct literal.
#[derive(Clone)]
pub struct RetryPolicy {
    max_attempts: Option<u32>,
    deadline: Option<Duration>,
    backoff: Backoff,
    retry_if: RetryPredicate,
}

impl fmt::Debug for RetryPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("deadline", &self.deadline)
            .field("backoff", &self.backoff)
            .finish_non_exhaustive()
    }
}

/// Retry a request that never reached a listener, or that reached one too busy
/// to serve it.
///
/// Deliberately **not** [`RequestError::NoClient`]: that means nothing is
/// running at all, which no amount of waiting fixes on its own. Callers that are
/// waiting for a client to appear - a launch, not a query - say so explicitly.
///
/// Deliberately **not** 4xx either. A 404 is an answer, and what it means
/// depends on the route: from `product-launcher` on a tray-idle client it means
/// "not yet", from a read query it means "give up". The caller knows which.
fn transient(outcome: &Result<Response, RequestError>) -> bool {
    match outcome {
        Err(RequestError::Transport(_)) => true,
        Ok(response) => response.status().is_server_error(),
        _ => false,
    }
}

impl RetryPolicy {
    /// One attempt, no retry. The default.
    pub fn none() -> Self {
        Self {
            max_attempts: Some(1),
            deadline: None,
            backoff: Backoff::Fixed(Duration::ZERO),
            retry_if: Arc::new(|_| false),
        }
    }

    /// At most `max` attempts, retrying transport failures and 5xx.
    pub fn attempts(max: u32) -> Self {
        Self {
            max_attempts: Some(max.max(1)),
            deadline: None,
            backoff: Backoff::Exponential {
                initial: Duration::from_millis(100),
                multiplier: 2.0,
                max: Duration::from_secs(1),
            },
            retry_if: Arc::new(transient),
        }
    }

    /// Retry until `deadline` elapses, on the same terms as [`Self::attempts`].
    pub fn deadline(deadline: Duration) -> Self {
        Self {
            max_attempts: None,
            deadline: Some(deadline),
            backoff: Backoff::Exponential {
                initial: Duration::from_millis(100),
                multiplier: 2.0,
                max: Duration::from_secs(1),
            },
            retry_if: Arc::new(transient),
        }
    }

    pub fn backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    pub fn max_attempts(mut self, max: u32) -> Self {
        self.max_attempts = Some(max.max(1));
        self
    }

    /// Replace the predicate that decides whether an outcome is worth another go.
    ///
    /// This is where a caller encodes what a status means *for its route* - the
    /// judgement the harness deliberately refuses to make on their behalf.
    pub fn retry_if(
        mut self,
        predicate: impl Fn(&Result<Response, RequestError>) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.retry_if = Arc::new(predicate);
        self
    }

    /// Whether to make another attempt, and how long to wait first.
    pub(crate) fn next_delay(
        &self,
        outcome: &Result<Response, RequestError>,
        attempt: u32,
        elapsed: Duration,
    ) -> Option<Duration> {
        if !(self.retry_if)(outcome) {
            return None;
        }
        if self.max_attempts.is_some_and(|max| attempt >= max) {
            return None;
        }

        let delay = self.backoff.delay_after(attempt);

        // Sleeping past the deadline and then trying anyway would overshoot the
        // budget the caller set, so a delay that would not fit ends the loop.
        if self
            .deadline
            .is_some_and(|deadline| elapsed + delay >= deadline)
        {
            return None;
        }
        Some(delay)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(status: u16) -> Result<Response, RequestError> {
        Ok(Response::new(
            crate::client::StatusCode::from_u16(status).unwrap(),
            String::new(),
        ))
    }

    fn transport() -> Result<Response, RequestError> {
        Err(RequestError::Transport("refused".into()))
    }

    #[test]
    fn the_default_policy_makes_one_attempt() {
        let policy = RetryPolicy::default();
        assert!(policy.next_delay(&transport(), 1, Duration::ZERO).is_none());
    }

    #[test]
    fn transport_failures_retry_and_success_does_not() {
        let policy = RetryPolicy::attempts(3);
        assert!(policy.next_delay(&transport(), 1, Duration::ZERO).is_some());
        assert!(policy.next_delay(&ok(200), 1, Duration::ZERO).is_none());
    }

    /// A 404 is an answer whose meaning is the caller's to decide, so the
    /// default predicate must not spend the budget guessing at it.
    #[test]
    fn client_errors_are_not_retried_by_default() {
        let policy = RetryPolicy::attempts(3);
        assert!(policy.next_delay(&ok(404), 1, Duration::ZERO).is_none());
        assert!(policy.next_delay(&ok(464), 1, Duration::ZERO).is_none());
        assert!(policy.next_delay(&ok(503), 1, Duration::ZERO).is_some());
    }

    /// Nothing is listening at all; waiting does not change that on its own.
    #[test]
    fn no_client_is_not_retried_by_default() {
        let policy = RetryPolicy::attempts(3);
        let outcome = Err(RequestError::NoClient);
        assert!(policy.next_delay(&outcome, 1, Duration::ZERO).is_none());
    }

    #[test]
    fn the_attempt_ceiling_ends_the_loop() {
        let policy = RetryPolicy::attempts(3);
        assert!(policy.next_delay(&transport(), 2, Duration::ZERO).is_some());
        assert!(policy.next_delay(&transport(), 3, Duration::ZERO).is_none());
    }

    /// A delay that would not fit inside the budget must not be slept off and
    /// then followed by an attempt the caller never agreed to wait for.
    #[test]
    fn a_deadline_that_the_next_delay_would_overshoot_ends_the_loop() {
        let policy = RetryPolicy::deadline(Duration::from_secs(1))
            .backoff(Backoff::Fixed(Duration::from_millis(400)));

        assert!(
            policy
                .next_delay(&transport(), 1, Duration::from_millis(100))
                .is_some()
        );
        assert!(
            policy
                .next_delay(&transport(), 1, Duration::from_millis(700))
                .is_none()
        );
    }

    #[test]
    fn exponential_backoff_grows_and_then_caps() {
        let backoff = Backoff::Exponential {
            initial: Duration::from_millis(100),
            multiplier: 2.0,
            max: Duration::from_millis(500),
        };

        assert_eq!(backoff.delay_after(1), Duration::from_millis(100));
        assert_eq!(backoff.delay_after(2), Duration::from_millis(200));
        assert_eq!(backoff.delay_after(3), Duration::from_millis(400));
        assert_eq!(backoff.delay_after(4), Duration::from_millis(500));
        assert_eq!(backoff.delay_after(9), Duration::from_millis(500));
    }

    #[test]
    fn a_custom_predicate_replaces_the_default() {
        let policy = RetryPolicy::attempts(3).retry_if(
            |outcome| matches!(outcome, Ok(response) if response.status().as_u16() == 404),
        );

        assert!(policy.next_delay(&ok(404), 1, Duration::ZERO).is_some());
        assert!(policy.next_delay(&transport(), 1, Duration::ZERO).is_none());
    }
}
