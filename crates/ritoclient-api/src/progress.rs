//! Progress reporting for a launch request.

use serde::Serialize;

/// Stage of a League launch request.
///
/// A launch is a single blocking call that can spend a minute inside one step -
/// waking a tray-idle client and waiting for it to load its launcher plugin.
/// Without these the caller cannot tell that wait apart from a hang, which is
/// the whole reason they exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum LaunchStage {
    /// Locating `RiotClientServices.exe` and checking what is already running.
    Resolving,
    /// Asking a running client to launch the product.
    HandingOff,
    /// Starting a Riot Client because none was running.
    ColdStart,
    /// Nudging a client that is idling in the tray.
    WakingClient,
    /// Waiting for that client to finish booting.
    WaitingForClient,
    /// The client accepted the request.
    Launched,
    /// League was already up; no request was sent.
    AlreadyRunning,
    /// The request failed; the error itself is reported separately.
    Error,
}

/// Progress of a League launch request.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchProgress {
    pub stage: LaunchStage,
    /// Seconds spent waiting for the client so far. Only meaningful during
    /// [`LaunchStage::WaitingForClient`]; zero everywhere else.
    pub waited_secs: u32,
    /// How long that wait may run before it gives up. Zero outside the wait.
    pub timeout_secs: u32,
}

impl LaunchProgress {
    /// A stage that involves no waiting.
    pub fn at(stage: LaunchStage) -> Self {
        Self {
            stage,
            waited_secs: 0,
            timeout_secs: 0,
        }
    }

    /// The wait for a booting client, with the numbers that make its progress
    /// bar determinate.
    pub fn waiting(waited_secs: u32, timeout_secs: u32) -> Self {
        Self {
            stage: LaunchStage::WaitingForClient,
            waited_secs,
            timeout_secs,
        }
    }
}

/// Receives launch progress.
///
/// Deliberately narrower than a general event sink: this crate has one thing to
/// report and no opinion about how a host surfaces it. The manager adapts this
/// to its own event registry; a CLI would print lines.
///
/// Called synchronously from the launching thread, so implementations must be
/// cheap and must not block.
pub trait LaunchObserver {
    fn on_progress(&self, progress: LaunchProgress);
}

/// An observer that drops everything, for callers that don't care.
pub struct NullObserver;

impl LaunchObserver for NullObserver {
    fn on_progress(&self, _progress: LaunchProgress) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend switches on these strings to pick a step and a label, so a
    /// renamed variant must not silently change what it serializes to.
    #[test]
    fn launch_stages_serialize_to_their_wire_names() {
        for (stage, expected) in [
            (LaunchStage::Resolving, "\"resolving\""),
            (LaunchStage::HandingOff, "\"handingOff\""),
            (LaunchStage::ColdStart, "\"coldStart\""),
            (LaunchStage::WakingClient, "\"wakingClient\""),
            (LaunchStage::WaitingForClient, "\"waitingForClient\""),
            (LaunchStage::Launched, "\"launched\""),
            (LaunchStage::AlreadyRunning, "\"alreadyRunning\""),
            (LaunchStage::Error, "\"error\""),
        ] {
            assert_eq!(serde_json::to_string(&stage).unwrap(), expected);
        }
    }

    /// The wait is the only determinate part of a launch; losing either number
    /// turns its progress bar back into a spinner.
    #[test]
    fn waiting_progress_carries_both_numbers() {
        let json = serde_json::to_value(LaunchProgress::waiting(12, 60)).unwrap();
        assert_eq!(json["stage"], "waitingForClient");
        assert_eq!(json["waitedSecs"], 12);
        assert_eq!(json["timeoutSecs"], 60);

        let json = serde_json::to_value(LaunchProgress::at(LaunchStage::HandingOff)).unwrap();
        assert_eq!(json["waitedSecs"], 0);
        assert_eq!(json["timeoutSecs"], 0);
    }

    #[test]
    fn null_observer_accepts_progress() {
        NullObserver.on_progress(LaunchProgress::at(LaunchStage::Resolving));
    }
}
