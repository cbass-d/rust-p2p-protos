//! Typed, multi-subscriber event bus over `tokio::sync::broadcast`.

use std::fmt;

use tokio::sync::broadcast;

/// A multi-subscriber, in-memory event bus.
pub(crate) struct EventBus<E: Clone + Send + 'static> {
    tx: broadcast::Sender<E>,
}

impl<E: Clone + Send + 'static> EventBus<E> {
    /// Build a bus with a fixed-capacity ring buffer.
    pub(crate) fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Non-blocking publish; silently drops when there are no subscribers.
    pub(crate) fn publish(&self, event: E) {
        let _ = self.tx.send(event);
    }

    /// Attach a new subscriber with its own independent lag position.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<E> {
        self.tx.subscribe()
    }
}

impl<E: Clone + Send + 'static> Clone for EventBus<E> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<E: Clone + Send + 'static> fmt::Debug for EventBus<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBus")
            .field("subscribers", &self.tx.receiver_count())
            .finish()
    }
}
