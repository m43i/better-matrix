//! One account's interaction history. A worker owns the counters and serializes store writes;
//! engine tasks send interactions and request snapshots, never share mutable counters.

use std::{collections::BTreeMap, sync::Arc};

use tokio::sync::{mpsc, oneshot};

use crate::{Account, Error, Event, EventSink};

const KEY: &[u8] = b"better-matrix/search-usage/v1";

/// Device-local interaction counts for one account, used within each search section.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ffi", derive(ts_rs::TS))]
pub struct SearchUsage {
    /// Room opens count once; sending a message counts three times.
    #[serde(default)]
    pub rooms: BTreeMap<String, u64>,
    /// Direct-chat interactions and explicit replies to people in any room.
    #[serde(default)]
    pub people: BTreeMap<String, u64>,
}

impl SearchUsage {
    /// How often this room has been used on this device.
    pub fn room(&self, id: &str) -> u64 {
        self.rooms.get(id).copied().unwrap_or(0)
    }

    /// How often this person has been contacted directly or replied to.
    pub fn person(&self, id: &str) -> u64 {
        self.people.get(id).copied().unwrap_or(0)
    }

    /// Prefer messages in frequently used rooms and from frequently contacted people.
    pub fn message(&self, room: &str, sender: &str) -> u64 {
        self.room(room).saturating_add(self.person(sender))
    }

    fn record(&mut self, interaction: Interaction) {
        increment(&mut self.rooms, interaction.room, interaction.weight);
        for person in interaction.people {
            increment(&mut self.people, person, interaction.weight);
        }
    }
}

fn increment(counts: &mut BTreeMap<String, u64>, id: String, weight: u64) {
    let count = counts.entry(id).or_default();
    *count = count.saturating_add(weight);
}

#[derive(Debug)]
struct Interaction {
    room: String,
    people: Vec<String>,
    weight: u64,
    reply_to: Option<String>,
}

#[derive(Debug)]
enum UsageCommand {
    Interact(Interaction),
    Snapshot(oneshot::Sender<SearchUsage>),
}

#[derive(Clone)]
pub(crate) struct Usage {
    send: mpsc::UnboundedSender<UsageCommand>,
}

impl Usage {
    pub fn start(account: Account, sink: Arc<dyn EventSink>) -> (Self, tokio::task::AbortHandle) {
        let (send, mut receive) = mpsc::unbounded_channel::<UsageCommand>();
        let worker = tokio::spawn(async move {
            let (mut usage, writable) = match load(&account).await {
                Ok(usage) => (usage, true),
                Err(error) => {
                    eprintln!("matrix-engine: could not read search usage: {error}");
                    (SearchUsage::default(), false)
                }
            };
            sink.emit(Event::SearchUsage { account: account.id().clone(), usage: usage.clone() });
            while let Some(command) = receive.recv().await {
                let mut interaction = match command {
                    UsageCommand::Interact(interaction) => interaction,
                    UsageCommand::Snapshot(reply) => {
                        let _ = reply.send(usage.clone());
                        continue;
                    }
                };
                if let Some(reply) = &interaction.reply_to
                    && let Some(sender) = reply_sender(&account, &interaction.room, reply).await
                    && sender != account.id().as_str()
                    && !interaction.people.contains(&sender)
                {
                    interaction.people.push(sender);
                }
                usage.record(interaction);
                sink.emit(Event::SearchUsage { account: account.id().clone(), usage: usage.clone() });
                if writable && let Err(error) = save(&account, &usage).await {
                    eprintln!("matrix-engine: could not save search usage: {error}");
                }
            }
        });
        (Self { send }, worker.abort_handle())
    }

    pub fn room(&self, account: &Account, room_id: &str, weight: u64, reply_to: Option<String>) {
        let Some(room) = matrix_sdk::ruma::RoomId::parse(room_id).ok().and_then(|id| account.client().get_room(&id))
        else {
            return;
        };
        let people = room
            .direct_targets()
            .iter()
            .filter_map(|target| target.as_user_id())
            .filter(|id| id.as_str() != account.id().as_str())
            .map(ToString::to_string)
            .collect();
        let _ =
            self.send.send(UsageCommand::Interact(Interaction { room: room_id.to_owned(), people, weight, reply_to }));
    }

    pub async fn snapshot(&self) -> SearchUsage {
        let (send, receive) = oneshot::channel();
        let _ = self.send.send(UsageCommand::Snapshot(send));
        receive.await.unwrap_or_default()
    }
}

async fn reply_sender(account: &Account, room: &str, reply: &str) -> Option<String> {
    use matrix_sdk::cross_process_lock::MappedCrossProcessLockState;
    let room = matrix_sdk::ruma::RoomId::parse(room).ok()?;
    let reply = matrix_sdk::ruma::EventId::parse(reply).ok()?;
    let lock = account.client().event_cache_store().lock().await.ok()?;
    let (MappedCrossProcessLockState::Clean(store) | MappedCrossProcessLockState::Dirty(store)) = lock;
    store.find_event(&room, &reply).await.ok()??.raw().get_field("sender").ok()?
}

async fn load(account: &Account) -> Result<SearchUsage, Error> {
    let stored =
        account.client().state_store().get_custom_value(KEY).await.map_err(|e| Error::Account(e.to_string()))?;
    stored.map_or_else(
        || Ok(SearchUsage::default()),
        |bytes| serde_json::from_slice(&bytes).map_err(|e| Error::Account(e.to_string())),
    )
}

async fn save(account: &Account, usage: &SearchUsage) -> Result<(), Error> {
    let bytes = serde_json::to_vec(usage).map_err(|e| Error::Account(e.to_string()))?;
    account.client().state_store().set_custom_value_no_read(KEY, bytes).await.map_err(|e| Error::Account(e.to_string()))
}

#[cfg(test)]
mod tests;
