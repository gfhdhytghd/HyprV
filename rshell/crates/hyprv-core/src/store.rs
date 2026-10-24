use std::sync::{Arc, Mutex};
use tokio::sync::watch;

/// A topic's revision only advances for an actual value change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot<T> {
    pub revision: u64,
    pub value: T,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff<T> {
    pub before: Snapshot<T>,
    pub after: Snapshot<T>,
}

struct Inner<T> {
    state: Mutex<Snapshot<T>>,
    values: watch::Sender<Snapshot<T>>,
    subscribers: watch::Sender<usize>,
}
#[derive(Clone)]
pub struct Topic<T>(Arc<Inner<T>>);
pub struct Subscription<T: Clone> {
    topic: Topic<T>,
    receiver: watch::Receiver<Snapshot<T>>,
    seen: Snapshot<T>,
}
impl<T: Clone + PartialEq> Topic<T> {
    pub fn new(value: T) -> Self {
        let initial = Snapshot { revision: 0, value };
        let (values, _) = watch::channel(initial.clone());
        let (subscribers, _) = watch::channel(0);
        Self(Arc::new(Inner {
            state: Mutex::new(initial),
            values,
            subscribers,
        }))
    }
    /// Serializes concurrent publishers; equal values emit no notification.
    pub fn publish(&self, value: T) -> Option<Diff<T>> {
        let mut state = self.0.state.lock().unwrap();
        if state.value == value {
            return None;
        }
        let before = state.clone();
        *state = Snapshot {
            revision: state.revision + 1,
            value,
        };
        self.0.values.send_replace(state.clone());
        Some(Diff {
            before,
            after: state.clone(),
        })
    }
    pub fn snapshot(&self) -> Snapshot<T> {
        self.0.state.lock().unwrap().clone()
    }
    pub fn subscribe(&self) -> Subscription<T> {
        let receiver = self.0.values.subscribe();
        self.0.subscribers.send_modify(|n| *n += 1);
        let seen = receiver.borrow().clone();
        Subscription {
            topic: self.clone(),
            receiver,
            seen,
        }
    }
    pub fn subscriber_count(&self) -> usize {
        *self.0.subscribers.borrow()
    }
    pub fn demand(&self) -> watch::Receiver<usize> {
        self.0.subscribers.subscribe()
    }
}
impl<T: Clone> Subscription<T> {
    /// Latest state semantics: slow consumers coalesce intermediate changes.
    pub fn current(&self) -> Snapshot<T> {
        self.receiver.borrow().clone()
    }
    pub async fn changed(&mut self) -> Result<Snapshot<T>, watch::error::RecvError> {
        self.receiver.changed().await?;
        self.seen = self.receiver.borrow_and_update().clone();
        Ok(self.seen.clone())
    }
    /// Diff from the last delivered revision, including any coalesced updates.
    pub async fn changed_diff(&mut self) -> Result<Diff<T>, watch::error::RecvError> {
        let before = self.seen.clone();
        let after = self.changed().await?;
        Ok(Diff { before, after })
    }
}
impl<T: Clone> Drop for Subscription<T> {
    fn drop(&mut self) {
        self.topic.0.subscribers.send_modify(|n| *n -= 1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn diffs_and_demand() {
        let topic = Topic::new(10);
        let mut sub = topic.subscribe();
        assert_eq!(topic.subscriber_count(), 1);
        assert!(topic.publish(10).is_none());
        let diff = topic.publish(20).unwrap();
        assert_eq!(diff.before.value, 10);
        assert_eq!(sub.changed().await.unwrap(), diff.after);
        drop(sub);
        assert_eq!(topic.subscriber_count(), 0);
    }
    #[test]
    fn publication_without_receivers_retains_latest() {
        let topic = Topic::new(false);
        topic.publish(true);
        assert!(topic.subscribe().current().value);
    }
}

/// Keys are marker types, so two domains with the same value type stay isolated.
pub trait TopicKey: 'static {
    type Value: Clone + PartialEq + Send + Sync + 'static;
}
#[derive(Default)]
pub struct StateStore {
    topics:
        Mutex<std::collections::HashMap<std::any::TypeId, Box<dyn std::any::Any + Send + Sync>>>,
}
impl StateStore {
    pub fn topic<K: TopicKey>(&self, initial: K::Value) -> Topic<K::Value> {
        let mut topics = self.topics.lock().unwrap();
        topics
            .entry(std::any::TypeId::of::<K>())
            .or_insert_with(|| Box::new(Topic::new(initial)))
            .downcast_ref::<Topic<K::Value>>()
            .expect("type-keyed topic invariant")
            .clone()
    }
}
