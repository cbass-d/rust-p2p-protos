use std::time::Instant;

use tokio_util::sync::{CancellationToken, WaitForCancellationFuture};

/// Manages the state of the running node
pub(crate) struct State {
    /// Flag for stopping the node
    quit: bool,

    /// Node killed by network command
    killed: bool,

    /// Flag for the status of the Kademlia bootstrap process
    bootstrapped: bool,

    /// Instant when the node started running
    start: Instant,

    /// `CancellationToken` that shared network
    cancellation_token: CancellationToken,
}

impl State {
    pub(crate) fn new(start: Instant, cancellation_token: CancellationToken) -> Self {
        Self {
            quit: false,
            killed: false,
            bootstrapped: false,
            start,
            cancellation_token,
        }
    }

    pub(crate) fn start(&self) -> Instant {
        self.start
    }

    pub(crate) fn stop(&mut self) {
        self.quit = true;
    }

    pub(crate) fn kill(&mut self) {
        self.killed = true;
    }

    pub(crate) fn bootstrap(&mut self) {
        self.bootstrapped = true;
    }

    pub(crate) fn stopped(&self) -> bool {
        self.quit
    }

    pub(crate) fn killed(&self) -> bool {
        self.killed
    }

    pub(crate) fn bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    pub(crate) fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        self.cancellation_token.cancelled()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use tokio_util::sync::CancellationToken;

    use crate::node::state::State;

    #[test]
    fn test_new_state() {
        let start = Instant::now();
        let state = State::new(start, CancellationToken::new());

        assert!(!state.killed());
        assert!(!state.bootstrapped());
        assert!(!state.stopped());
        assert_eq!(state.start(), start);
    }

    #[test]
    fn test_stopped() {
        let start = Instant::now();
        let mut state = State::new(start, CancellationToken::new());

        state.stop();

        assert!(state.stopped());
        assert!(!state.killed());
        assert!(!state.bootstrapped());
    }

    #[test]
    fn test_killed() {
        let start = Instant::now();
        let mut state = State::new(start, CancellationToken::new());

        state.kill();

        assert!(state.killed());
        assert!(!state.bootstrapped());
        assert!(!state.stopped());
    }

    #[test]
    fn test_bootstrapped() {
        let start = Instant::now();
        let mut state = State::new(start, CancellationToken::new());

        state.bootstrap();

        assert!(state.bootstrapped());
        assert!(!state.killed());
        assert!(!state.stopped());
    }
}
