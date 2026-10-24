//! Credentials never enter serde state or Debug output; prompt metadata is safe to render.
use crate::{
    secret::{Fields, Secret, Zeroize, Zeroizing},
    Result,
};
use hyprv_core::store::Topic;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::oneshot;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Kind {
    NetworkSecrets {
        connection: String,
        setting: String,
        hints: Vec<String>,
        new_secrets: bool,
    },
    Enterprise {
        ssid: String,
    },
    Pin {
        device: String,
    },
    Passkey {
        device: String,
    },
    Confirmation {
        device: String,
        passkey: u32,
    },
    Authorization {
        device: String,
        service: Option<String>,
    },
    DisplayPin {
        device: String,
        pin: String,
    },
    DisplayPasskey {
        device: String,
        passkey: u32,
        entered: u16,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    pub id: u64,
    pub kind: Kind,
}
/// No Debug/Serialize implementation: this value may contain passwords.
/// Secret contents are wiped when the reply is dropped; consumers move them
/// out with [`Reply::take_fields`] / [`Reply::take_pin`] without copying.
pub enum Reply {
    Reject,
    Accept,
    Pin(String),
    Passkey(u32),
    Fields(BTreeMap<String, String>),
}
impl Reply {
    /// Move the field values into zeroizing buffers (no copy of the bytes).
    pub fn take_fields(&mut self) -> Option<Fields> {
        match self {
            Reply::Fields(map) => Some(
                std::mem::take(map)
                    .into_iter()
                    .map(|(k, v)| (k, Zeroizing::new(v)))
                    .collect(),
            ),
            _ => None,
        }
    }
    pub fn take_pin(&mut self) -> Option<Secret> {
        match self {
            Reply::Pin(pin) => Some(Zeroizing::new(std::mem::take(pin))),
            _ => None,
        }
    }
    pub fn is_accept(&self) -> bool {
        matches!(self, Reply::Accept)
    }
}
impl Drop for Reply {
    fn drop(&mut self) {
        match self {
            Reply::Pin(pin) => pin.zeroize(),
            Reply::Fields(map) => map.values_mut().for_each(Zeroize::zeroize),
            _ => {}
        }
    }
}
/// Why a prompt ended without a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptEnd {
    /// Withdrawn by the provider/service (CancelGetSecrets, Agent1.Cancel,
    /// owner change, shutdown).
    Cancelled,
    TimedOut,
}
impl std::fmt::Display for PromptEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PromptEnd::Cancelled => "prompt cancelled",
            PromptEnd::TimedOut => "prompt timed out",
        })
    }
}
struct Entry {
    prompt: Prompt,
    reply: Option<oneshot::Sender<Reply>>,
}
struct Inner {
    next: AtomicU64,
    entries: Mutex<HashMap<u64, Entry>>,
    topic: Topic<Vec<Prompt>>,
}
#[derive(Clone)]
pub struct Broker(Arc<Inner>);
impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}
impl Broker {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            next: AtomicU64::new(1),
            entries: Mutex::new(HashMap::new()),
            topic: Topic::new(vec![]),
        }))
    }
    pub fn topic(&self) -> Topic<Vec<Prompt>> {
        self.0.topic.clone()
    }
    fn publish(&self, entries: &HashMap<u64, Entry>) {
        let mut values: Vec<_> = entries.values().map(|e| e.prompt.clone()).collect();
        values.sort_by_key(|p| p.id);
        self.0.topic.publish(values);
    }
    pub fn respond(&self, id: u64, reply: Reply) -> Result<()> {
        let mut entries = self.0.entries.lock().unwrap();
        let entry = entries.remove(&id).ok_or("prompt expired or cancelled")?;
        self.publish(&entries);
        entry
            .reply
            .ok_or("display-only prompt")?
            .send(reply)
            .map_err(|_| "prompt cancelled".into())
    }
    pub async fn request(&self, kind: Kind) -> std::result::Result<Reply, PromptEnd> {
        let id = self.0.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut entries = self.0.entries.lock().unwrap();
            entries.insert(
                id,
                Entry {
                    prompt: Prompt { id, kind },
                    reply: Some(tx),
                },
            );
            self.publish(&entries);
        }
        // A guard removes prompts even if the D-Bus method future is cancelled.
        let _guard = Guard {
            broker: self.clone(),
            id,
        };
        tokio::time::timeout(Duration::from_secs(120), rx)
            .await
            .map_err(|_| PromptEnd::TimedOut)?
            .map_err(|_| PromptEnd::Cancelled)
    }
    pub fn display(&self, kind: Kind) {
        self.cancel_where(|p| {
            matches!(
                p.kind,
                Kind::DisplayPin { .. } | Kind::DisplayPasskey { .. }
            )
        });
        let id = self.0.next.fetch_add(1, Ordering::Relaxed);
        let mut entries = self.0.entries.lock().unwrap();
        entries.insert(
            id,
            Entry {
                prompt: Prompt { id, kind },
                reply: None,
            },
        );
        self.publish(&entries);
    }
    pub fn cancel_where(&self, predicate: impl Fn(&Prompt) -> bool) {
        let mut entries = self.0.entries.lock().unwrap();
        entries.retain(|_, e| !predicate(&e.prompt));
        self.publish(&entries);
    }
    pub fn cancel_all(&self) {
        self.cancel_where(|_| true);
    }
}
struct Guard {
    broker: Broker,
    id: u64,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.broker.cancel_where(|p| p.id == self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn response_cancellation_and_redaction() {
        let broker = Broker::new();
        let mut sub = broker.topic().subscribe();
        let b = broker.clone();
        let request = tokio::spawn(async move {
            b.request(Kind::Passkey {
                device: "/dev1".into(),
            })
            .await
        });
        let p = sub.changed().await.unwrap().value.remove(0);
        broker.respond(p.id, Reply::Passkey(123456)).unwrap();
        assert!(matches!(
            request.await.unwrap().unwrap(),
            Reply::Passkey(123456)
        ));
        assert!(broker.topic().snapshot().value.is_empty());
        let b = broker.clone();
        let request = tokio::spawn(async move {
            b.request(Kind::Pin {
                device: "/dev2".into(),
            })
            .await
        });
        while broker.topic().snapshot().value.is_empty() {
            tokio::task::yield_now().await;
        }
        broker.cancel_all();
        assert!(request.await.unwrap().is_err());
        assert!(broker.respond(p.id, Reply::Accept).is_err());
    }
    #[tokio::test]
    async fn cancelled_future_removes_prompt_and_display_updates_coalesce() {
        let b = Broker::new();
        let cloned = b.clone();
        let task = tokio::spawn(async move {
            cloned
                .request(Kind::Enterprise {
                    ssid: "test".into(),
                })
                .await
        });
        while b.topic().snapshot().value.is_empty() {
            tokio::task::yield_now().await;
        }
        task.abort();
        let _ = task.await;
        assert!(b.topic().snapshot().value.is_empty());
        b.display(Kind::DisplayPasskey {
            device: "dev".into(),
            passkey: 42,
            entered: 0,
        });
        b.display(Kind::DisplayPasskey {
            device: "dev".into(),
            passkey: 42,
            entered: 2,
        });
        assert_eq!(b.topic().snapshot().value.len(), 1);
        b.cancel_all();
    }
}
