use std::collections::VecDeque;
use tracing::debug;

use crate::tui::app::Action;

/// FIFO queue of pending [`Action`]s.
#[derive(Default, Debug)]
pub(crate) struct ActionQueue {
    queue: VecDeque<Action>,
}

impl ActionQueue {
    pub fn push(&mut self, action: Action) {
        debug!(target: "app::actions", action=?action, "new action pushed to queue");
        self.queue.push_back(action);
    }

    pub fn next(&mut self) -> Option<Action> {
        self.queue.pop_front()
    }

    //pub fn is_empty(&self) -> bool {
    //    self.queue.is_empty()
    //}
}
