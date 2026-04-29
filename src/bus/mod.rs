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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::broadcast::error::{RecvError, TryRecvError};

    use crate::bus::EventBus;

    #[test]
    fn test_clone_shares_channel() {
        let bus = EventBus::<i32>::new(2);
        let cloned = bus.clone();
        let mut rx = bus.subscribe();

        cloned.publish(1);

        assert_eq!(rx.try_recv().unwrap(), 1);
    }

    #[test]
    fn test_distinct_subscribers() {
        let bus = EventBus::<i32>::new(2);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(1);
        bus.publish(2);

        assert_eq!(rx1.try_recv().unwrap(), 1);
        assert_eq!(rx1.try_recv().unwrap(), 2);

        assert_eq!(rx2.try_recv().unwrap(), 1);
        assert_eq!(rx2.try_recv().unwrap(), 2);
    }

    #[test]
    fn test_no_replay() {
        let bus = EventBus::<i32>::new(2);

        let mut rx = bus.subscribe();

        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty))
    }

    #[tokio::test]
    async fn test_slow_subscriber() {
        let bus = EventBus::<i32>::new(1);
        let mut rx = bus.subscribe();

        bus.publish(1);
        bus.publish(2);

        let lagged = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("recv timeout");

        assert_eq!(lagged, Err(RecvError::Lagged(1)));

        let remaining = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("recv timeout");

        assert_eq!(remaining, Ok(2));
    }
}
