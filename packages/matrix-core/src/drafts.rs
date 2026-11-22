//! Drafts: what the user was writing in a room or thread and left unsent, kept in this device's
//! store (the SDK's composer drafts) so it is still there after leaving, or after a restart.
//! Only the text and its formatting are kept: replies and attachments live with the app's
//! composer while it runs.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use matrix_sdk::{ComposerDraft, ComposerDraftType};
use tokio::sync::{mpsc, oneshot};

use crate::Error;
use crate::account::Account;
use crate::model::TimelineKey;

type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Jobs run one at a time in the order they were asked for. One account's draft reads and
/// writes go through one, so a write never lands after a later one (a draft written back after
/// the clear that followed its send) and a read sees every write asked for before it; its room
/// tags, unread marks and keywords go through another (`Account::organising`).
#[derive(Clone, Debug, Default)]
pub(crate) struct DraftQueue(Arc<OnceLock<mpsc::UnboundedSender<Job>>>);

impl DraftQueue {
    /// Queues `job` at once (not when the returned future is first polled); the answer comes once
    /// every job queued before it has run. Call within the engine's runtime.
    pub(crate) fn run<T, F>(&self, job: F) -> impl Future<Output = Result<T, Error>> + use<T, F>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, Error>> + Send + 'static,
    {
        let queue = self.0.get_or_init(|| {
            let (queue, mut jobs) = mpsc::unbounded_channel::<Job>();
            tokio::spawn(async move {
                while let Some(job) = jobs.recv().await {
                    job.await;
                }
            });
            queue
        });
        let (answer, answered) = oneshot::channel();
        let _ = queue.send(Box::pin(async move {
            let _ = answer.send(job.await);
        }));
        async move { answered.await.unwrap_or(Err(Error::Cancelled)) }
    }
}

/// A message being written and not sent yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Draft {
    /// The text.
    pub body: String,
    /// Its formatting as Matrix HTML, when it has any.
    pub html: Option<String>,
}

impl Account {
    /// Keeps `draft` for the timeline `key`, replacing what was kept; `None` forgets it.
    pub async fn save_draft(&self, key: &TimelineKey, draft: Option<Draft>) -> Result<(), Error> {
        let room = self.room(&key.room_id)?;
        let thread = key.thread.as_deref().map(matrix_sdk::ruma::EventId::parse).transpose()?;
        match draft.filter(|draft| !draft.body.trim().is_empty()) {
            Some(Draft { body, html }) => {
                let draft = ComposerDraft {
                    plain_text: body,
                    html_text: html,
                    draft_type: ComposerDraftType::NewMessage,
                    attachments: Vec::new(),
                };
                room.save_composer_draft(draft, thread.as_deref()).await?;
            }
            None => room.clear_composer_draft(thread.as_deref()).await?,
        }
        Ok(())
    }

    /// The draft kept for the timeline `key`, if any.
    pub async fn load_draft(&self, key: &TimelineKey) -> Result<Option<Draft>, Error> {
        let room = self.room(&key.room_id)?;
        let thread = key.thread.as_deref().map(matrix_sdk::ruma::EventId::parse).transpose()?;
        let draft = room.load_composer_draft(thread.as_deref()).await?;
        Ok(draft
            .filter(|draft| !draft.plain_text.trim().is_empty())
            .map(|draft| Draft { body: draft.plain_text, html: draft.html_text }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn account() -> (crate::test_server::Server, Account) {
        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        (server, account)
    }

    #[tokio::test]
    async fn queued_draft_changes_should_land_in_the_order_they_were_asked_for() {
        let (_server, account) = account().await;
        let key = TimelineKey::room("!room:example.org");
        let queue = DraftQueue::default();
        let (slow, slow_key) = (account.clone(), key.clone());
        let save = queue.run(async move {
            // The write asked for first is slow to start; the clear asked after it must still
            // land last.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            slow.save_draft(&slow_key, Some(Draft { body: "sent already".into(), html: None })).await
        });
        let (fast, fast_key) = (account.clone(), key.clone());
        let clear = queue.run(async move { fast.save_draft(&fast_key, None).await });
        let (saved, cleared) = tokio::join!(save, clear);
        saved.expect("save");
        cleared.expect("clear");
        assert_eq!(account.load_draft(&key).await.expect("load"), None);
    }

    #[tokio::test]
    async fn a_saved_draft_should_load_again() {
        let (_server, account) = account().await;
        let key = TimelineKey::room("!room:example.org");
        let draft = Draft { body: "Half a thought".into(), html: Some("<b>Half</b> a thought".into()) };
        account.save_draft(&key, Some(draft.clone())).await.expect("save");
        assert_eq!(account.load_draft(&key).await.expect("load"), Some(draft));
    }

    #[tokio::test]
    async fn a_threads_draft_should_be_kept_apart_from_its_rooms() {
        let (_server, account) = account().await;
        let room = TimelineKey::room("!room:example.org");
        let thread = TimelineKey::thread("!room:example.org", "$root");
        account.save_draft(&thread, Some(Draft { body: "In the thread".into(), html: None })).await.expect("save");
        assert_eq!(account.load_draft(&room).await.expect("load"), None);
        assert_eq!(account.load_draft(&thread).await.expect("load").map(|d| d.body).as_deref(), Some("In the thread"));
    }

    #[tokio::test]
    async fn saving_no_draft_should_forget_the_kept_one() {
        let (_server, account) = account().await;
        let key = TimelineKey::room("!room:example.org");
        account.save_draft(&key, Some(Draft { body: "Gone soon".into(), html: None })).await.expect("save");
        account.save_draft(&key, None).await.expect("clear");
        assert_eq!(account.load_draft(&key).await.expect("load"), None);
    }

    #[tokio::test]
    async fn a_blank_draft_should_not_be_kept() {
        let (_server, account) = account().await;
        let key = TimelineKey::room("!room:example.org");
        account.save_draft(&key, Some(Draft { body: "Kept".into(), html: None })).await.expect("save");
        account.save_draft(&key, Some(Draft { body: "  \n".into(), html: None })).await.expect("save blank");
        assert_eq!(account.load_draft(&key).await.expect("load"), None);
    }
}
