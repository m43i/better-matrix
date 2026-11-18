//! Drafts: what the user was writing in a room or thread and left unsent, kept in this device's
//! store (the SDK's composer drafts) so it is still there after leaving, or after a restart.
//! Only the text and its formatting are kept: replies and attachments live with the app's
//! composer while it runs.

use matrix_sdk::{ComposerDraft, ComposerDraftType};

use crate::Error;
use crate::account::Account;
use crate::model::TimelineKey;

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
