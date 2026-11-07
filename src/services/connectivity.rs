//! Whether GitHub can be reached, learned from the gateway's own requests.
//!
//! A request that fails without an answer — no connection, no DNS, a dropped
//! connection, no reply in time — marks GitHub unreachable; any other outcome,
//! an HTTP error status included, marks it reachable. While unreachable, a probe of
//! `/rate_limit` (which costs no rate limit) retries with a growing pause, so
//! the app notices the network coming back even when nothing else is asking.
//!
//! One instance lives in `AppServices` and is shared by every live gateway,
//! including the copies detail panes keep and the gateways that replace each
//! other on a new sign-in, so every window follows one state.
//!
//! The state moves under one lock. Each move back to reachable starts a new
//! epoch, and a failure from a request that started in an earlier epoch is
//! ignored: a request stuck in a 30 s timeout while the network came back must
//! not take the app offline again.

use crate::services::api::{GitHubClient, GitHubError};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

const FIRST_PROBE_DELAY: Duration = Duration::from_secs(10);
const MAX_PROBE_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct State {
    offline: bool,
    probing: bool,
    epoch: u64,
    /// Bumped by `reset`, so a probe from the session that ended stops at its
    /// next wake instead of carrying on with that session's client.
    probe_generation: u64,
}

pub struct Connectivity {
    state: Mutex<State>,
    changes: watch::Sender<bool>,
}

/// When a request started, in reachability epochs.
#[derive(Debug, Clone, Copy)]
pub struct RequestEpoch(u64);

impl Connectivity {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            changes: watch::Sender::new(false),
        })
    }

    pub fn is_offline(&self) -> bool {
        self.state.lock().offline
    }

    /// Follows [`is_offline`](Self::is_offline) as it changes.
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.changes.subscribe()
    }

    pub fn begin_request(&self) -> RequestEpoch {
        RequestEpoch(self.state.lock().epoch)
    }

    /// Back to reachable, and any running probe stops at its next wake: for
    /// sign-out and demo mode, where nothing is being reached.
    pub fn reset(&self) {
        let mut state = self.state.lock();
        state.probing = false;
        state.probe_generation += 1;
        if state.offline {
            state.offline = false;
            state.epoch += 1;
            self.changes.send_replace(false);
        }
    }

    pub(crate) fn observe<T>(
        self: &Arc<Self>,
        result: &Result<T, GitHubError>,
        started: RequestEpoch,
        client: &GitHubClient,
    ) {
        match result {
            Err(error) if error.is_offline() => {
                let mut state = self.state.lock();
                if started.0 != state.epoch {
                    // Started before GitHub was last seen coming back.
                    return;
                }
                if !state.offline {
                    state.offline = true;
                    self.changes.send_replace(true);
                }
                self.start_probe(&mut state, client.clone(), FIRST_PROBE_DELAY);
            }
            _ => self.mark_reachable(),
        }
    }

    /// Marks GitHub unreachable and starts the probe with `first_delay` as its
    /// first pause — for tests of the recovery path, which cannot make a live
    /// server unreachable on demand.
    pub(crate) fn simulate_offline(self: &Arc<Self>, client: &GitHubClient, first_delay: Duration) {
        let mut state = self.state.lock();
        if !state.offline {
            state.offline = true;
            self.changes.send_replace(true);
        }
        self.start_probe(&mut state, client.clone(), first_delay);
    }

    fn mark_reachable(&self) {
        let mut state = self.state.lock();
        if state.offline {
            state.offline = false;
            state.epoch += 1;
            self.changes.send_replace(false);
        }
    }

    fn start_probe(
        self: &Arc<Self>,
        state: &mut State,
        client: GitHubClient,
        first_delay: Duration,
    ) {
        if state.probing {
            return;
        }
        // Usually called on a Tokio worker, but a test may mark the state from
        // the GTK thread; the global runtime serves both.
        let Some(handle) = crate::runtime::try_handle() else {
            return;
        };
        state.probing = true;
        let generation = state.probe_generation;
        // Weak, so services dropped in a test do not keep probing.
        let connectivity = Arc::downgrade(self);
        let mut delay = first_delay;
        handle.spawn(async move {
            loop {
                tokio::time::sleep(delay).await;
                let Some(this) = connectivity.upgrade() else {
                    return;
                };
                if this.finish_probe_if_reachable(generation, None) {
                    return;
                }
                let reachable = client.probe().await.is_ok();
                if this.finish_probe_if_reachable(generation, Some(reachable)) {
                    return;
                }
                delay = (delay * 2).min(MAX_PROBE_DELAY);
            }
        });
    }

    /// Ends the probe if GitHub is already reachable, or if `probe_answered`
    /// says it just answered. Both checks and the state change happen under
    /// the lock, so a failure observed meanwhile cannot be lost.
    fn finish_probe_if_reachable(&self, generation: u64, probe_answered: Option<bool>) -> bool {
        let mut state = self.state.lock();
        if state.probe_generation != generation {
            // Reset since: a newer probe, if any, owns `probing`.
            return true;
        }
        if !state.offline {
            state.probing = false;
            return true;
        }
        if probe_answered == Some(true) {
            state.offline = false;
            state.epoch += 1;
            state.probing = false;
            self.changes.send_replace(false);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offline_error() -> GitHubError {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime
            .block_on(reqwest::get("http://127.0.0.1:1"))
            .expect_err("nothing listens")
            .into()
    }

    fn client() -> GitHubClient {
        GitHubClient::with_base_url("http://127.0.0.1:1", None).expect("client")
    }

    #[test]
    fn a_failure_from_before_the_last_recovery_is_ignored() {
        let connectivity = Connectivity::new();
        let error: Result<(), _> = Err(offline_error());

        let stale = connectivity.begin_request();
        // GitHub goes away, then comes back while `stale` is still waiting.
        connectivity.observe(&error, connectivity.begin_request(), &client());
        assert!(connectivity.is_offline());
        connectivity.observe(
            &Ok::<(), GitHubError>(()),
            connectivity.begin_request(),
            &client(),
        );
        assert!(!connectivity.is_offline());

        connectivity.observe(&error, stale, &client());
        assert!(
            !connectivity.is_offline(),
            "a request that started before the recovery cannot undo it"
        );
    }

    #[test]
    fn an_http_error_means_reachable() {
        let connectivity = Connectivity::new();
        connectivity.observe(
            &Err::<(), _>(offline_error()),
            connectivity.begin_request(),
            &client(),
        );
        assert!(connectivity.is_offline());
        connectivity.observe(
            &Err::<(), _>(GitHubError::NotFound),
            connectivity.begin_request(),
            &client(),
        );
        assert!(!connectivity.is_offline());
    }

    #[test]
    fn reset_retires_the_running_probe() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let _context = runtime.enter();
        let connectivity = Connectivity::new();
        connectivity.simulate_offline(&client(), Duration::from_secs(3600));
        assert!(connectivity.state.lock().probing);

        connectivity.reset();
        // The next session's failure starts its own probe at once rather than
        // waiting on the old loop, which would use the old session's client.
        connectivity.simulate_offline(&client(), Duration::from_secs(3600));
        let state = connectivity.state.lock();
        assert!(state.probing);
        assert_eq!(state.probe_generation, 1);
    }

    #[test]
    fn reset_ends_the_offline_state() {
        let connectivity = Connectivity::new();
        connectivity.observe(
            &Err::<(), _>(offline_error()),
            connectivity.begin_request(),
            &client(),
        );
        let changes = connectivity.subscribe();
        connectivity.reset();
        assert!(!connectivity.is_offline());
        assert!(changes.has_changed().expect("sender alive"));
    }
}
