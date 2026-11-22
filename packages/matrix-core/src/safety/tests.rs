use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;

/// A homeserver whose syncs bring `account_data`, keeping account data as it is written (and
/// answering 404 for events never written), recording every write as (request line, body).
struct Homeserver {
    server: crate::test_server::Server,
    writes: Arc<Mutex<Vec<(String, Value)>>>,
}

impl Homeserver {
    fn new(account_data: Value) -> Self {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let captured = writes.clone();
        let stored: Arc<Mutex<std::collections::HashMap<String, Value>>> = Arc::default();
        let server = crate::test_server::Server::new(move |request| {
            let line = request.lines().next().unwrap_or_default().to_owned();
            let path = line.split(' ').nth(1).unwrap_or_default().to_owned();
            if line.starts_with("PUT") || line.starts_with("POST") && path.contains("/report") {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                let body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                if path.contains("/account_data/") {
                    stored.lock().unwrap().insert(path, body.clone());
                }
                captured.lock().unwrap().push((line, body));
                return json!({});
            }
            if line.starts_with("GET") && path.contains("/account_data/") {
                return stored.lock().unwrap().get(&path).cloned().unwrap_or_else(
                    || json!({ crate::test_server::NOT_FOUND_STATUS: true, "errcode": "M_NOT_FOUND", "error": "none" }),
                );
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["account_data"] = json!({ "events": account_data.clone() });
            }
            response
        });
        Self { server, writes }
    }

    async fn synced_account(&self) -> Account {
        let account = Account::for_test(&self.server.url()).await;
        let settings = matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO);
        account.client().sync_once(settings).await.unwrap();
        account
    }

    /// The bodies of the writes whose request line contains `path`.
    fn sent(&self, path: &str) -> Vec<Value> {
        let writes = self.writes.lock().unwrap();
        writes.iter().filter(|(line, _)| line.contains(path)).map(|(_, body)| body.clone()).collect()
    }
}

#[test]
fn an_account_without_choices_should_show_everything() {
    assert_eq!(read(&Value::Null, &Value::Null, &Value::Null), Safety::default());
    assert!(Safety::default().shows_media(true, false));
}

#[test]
fn ignored_people_should_read_sorted() {
    let ignored = json!({ "ignored_users": { "@zed:example.org": {}, "@ada:example.org": {} } });
    let safety = read(&ignored, &Value::Null, &Value::Null);
    assert_eq!(safety.ignored, vec!["@ada:example.org".to_owned(), "@zed:example.org".to_owned()]);
    assert!(safety.ignores("@zed:example.org"));
}

#[test]
fn a_media_choice_another_client_made_should_be_read() {
    let media = json!({ "media_previews": "private", "invite_avatars": "off" });
    let safety = read(&Value::Null, &media, &Value::Null);
    assert_eq!(safety.media, ShowMedia::Private);
    assert!(!safety.invite_pictures);
}

#[test]
fn an_unknown_media_choice_should_hide_as_msc4278_asks() {
    let media = json!({ "media_previews": "sometimes", "invite_avatars": 3 });
    let safety = read(&Value::Null, &media, &Value::Null);
    assert_eq!(safety.media, ShowMedia::Never);
    assert!(!safety.invite_pictures);
}

#[test]
fn a_media_choice_without_its_fields_should_show() {
    let safety = read(&Value::Null, &json!({ "other": 1 }), &Value::Null);
    assert_eq!(safety.media, ShowMedia::Always);
    assert!(safety.invite_pictures);
}

#[test]
fn media_in_private_rooms_only_should_hide_public_rooms_media_but_not_the_users_own() {
    let safety = Safety { media: ShowMedia::Private, ..Safety::default() };
    assert!(safety.shows_media(false, false));
    assert!(!safety.shows_media(true, false));
    assert!(safety.shows_media(true, true));
}

#[test]
fn hiding_stranger_invites_should_hide_only_strangers() {
    let safety = Safety { hide_stranger_invites: true, ..Safety::default() };
    let invitation = crate::Invitation {
        room_id: "!spam:example.org".into(),
        name: "Free crypto".into(),
        topic: None,
        avatar: None,
        inviter: None,
        space: false,
        kind: crate::RoomKind::Group,
        stranger: true,
    };
    assert!(!safety.shows_invitation(&invitation));
    assert!(safety.shows_invitation(&crate::Invitation { stranger: false, ..invitation }));
}

#[test]
fn ignoring_and_unignoring_should_keep_the_list_sorted_without_repeats() {
    let ignore = |user_id: &str, ignored| SafetyChange::Ignore { user_id: user_id.into(), ignored };
    let safety = Safety::default()
        .with(&ignore("@zed:example.org", true))
        .with(&ignore("@ada:example.org", true))
        .with(&ignore("@ada:example.org", true));
    assert_eq!(safety.ignored, vec!["@ada:example.org".to_owned(), "@zed:example.org".to_owned()]);
    assert_eq!(safety.with(&ignore("@zed:example.org", false)).ignored, vec!["@ada:example.org".to_owned()]);
}

#[test]
fn a_media_change_should_keep_fields_another_client_set() {
    let content = with_media(json!({ "invite_avatars": "off", "extra": 1 }), Some(ShowMedia::Never), None);
    assert_eq!(content, json!({ "invite_avatars": "off", "extra": 1, "media_previews": "off" }));
}

#[test]
fn a_sync_carrying_any_safety_event_should_count_as_a_change() {
    let event = |kind: &str| Raw::new(&json!({ "type": kind, "content": {} })).unwrap().cast_unchecked();
    assert!(changed_in(&[event("io.element.msc4278.media_preview_config")]));
    assert!(changed_in(&[event("m.ignored_user_list")]));
    assert!(!changed_in(&[event("m.direct")]));
}

#[tokio::test]
async fn the_specs_media_choice_should_win_over_the_unstable_one() {
    let server = Homeserver::new(json!([
        { "type": MEDIA_UNSTABLE, "content": { "media_previews": "off" } },
        { "type": MEDIA, "content": { "media_previews": "private" } },
        { "type": INVITES, "content": { "hide_strangers": true } },
    ]));
    let safety = server.synced_account().await.safety().await.unwrap();
    assert_eq!(safety.media, ShowMedia::Private);
    assert!(safety.hide_stranger_invites);
}

#[tokio::test]
async fn choosing_where_media_shows_should_save_it_under_the_name_other_clients_write() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    let safety = account.change_safety(&SafetyChange::Media(ShowMedia::Never)).await.unwrap();
    assert_eq!(safety.media, ShowMedia::Never);
    assert_eq!(server.sent(&format!("/account_data/{MEDIA_UNSTABLE}"))[0]["media_previews"], "off");
    assert!(server.sent(&format!("/account_data/{MEDIA}")).is_empty());
}

#[tokio::test]
async fn a_media_choice_should_also_update_the_specs_copy_when_one_exists() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    account.save_account_data(MEDIA, &json!({ "invite_avatars": "off" })).await.unwrap();
    let safety = account.change_safety(&SafetyChange::Media(ShowMedia::Private)).await.unwrap();
    assert_eq!(safety.media, ShowMedia::Private);
    assert!(!safety.invite_pictures);
    let unstable = server.sent(&format!("/account_data/{MEDIA_UNSTABLE}"));
    assert_eq!(unstable, vec![json!({ "invite_avatars": "off", "media_previews": "private" })]);
}

#[tokio::test]
async fn changes_made_before_a_sync_should_all_hold() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    let ignore = |user_id: &str| SafetyChange::Ignore { user_id: user_id.into(), ignored: true };
    let (one, two) = (ignore("@one:spam.example"), ignore("@two:spam.example"));
    let (first, second) = tokio::join!(account.change_safety(&one), account.change_safety(&two));
    first.unwrap();
    let safety = second.unwrap();
    assert_eq!(safety.ignored, vec!["@one:spam.example".to_owned(), "@two:spam.example".to_owned()]);
}

#[tokio::test]
async fn the_user_should_not_be_able_to_ignore_themselves() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    let me = account.client().user_id().unwrap().to_string();
    assert!(account.change_safety(&SafetyChange::Ignore { user_id: me, ignored: true }).await.is_err());
    assert!(server.sent("/account_data/").is_empty());
}

#[tokio::test]
async fn stopping_ignoring_someone_should_keep_the_others() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    for user_id in ["@one:spam.example", "@two:spam.example"] {
        account.change_safety(&SafetyChange::Ignore { user_id: user_id.into(), ignored: true }).await.unwrap();
    }
    let change = SafetyChange::Ignore { user_id: "@one:spam.example".into(), ignored: false };
    let safety = account.change_safety(&change).await.unwrap();
    assert_eq!(safety.ignored, vec!["@two:spam.example".to_owned()]);
}

#[tokio::test]
async fn an_invitation_should_stay_hidden_by_the_servers_choice_even_when_this_device_has_none() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    account.save_account_data(INVITES, &json!({ "hide_strangers": true })).await.unwrap();
    let stranger = UserId::parse("@prizes:spam.example").unwrap();
    assert!(hides_invite_from(account.client(), &stranger).await);
}

#[tokio::test]
async fn an_invitation_from_someone_sharing_a_room_should_not_be_hidden() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    account.save_account_data(INVITES, &json!({ "hide_strangers": true })).await.unwrap();
    // Ada joined `!room:example.org` in the test server's sync.
    let ada = UserId::parse("@a:example.org").unwrap();
    assert!(!hides_invite_from(account.client(), &ada).await);
}

#[tokio::test]
async fn hiding_stranger_invites_should_save_it_in_the_accounts_data() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    let safety = account.change_safety(&SafetyChange::HideStrangerInvites(true)).await.unwrap();
    assert!(safety.hide_stranger_invites);
    assert_eq!(server.sent(&format!("/account_data/{INVITES}")), vec![json!({ "hide_strangers": true })]);
}

#[tokio::test]
async fn ignoring_someone_should_add_them_to_the_ignored_list() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    let change = SafetyChange::Ignore { user_id: "@spam:example.org".into(), ignored: true };
    let safety = account.change_safety(&change).await.unwrap();
    assert_eq!(safety.ignored, vec!["@spam:example.org".to_owned()]);
    let sent = server.sent(&format!("/account_data/{IGNORED}"));
    assert!(sent[0]["ignored_users"]["@spam:example.org"].is_object());
}

#[tokio::test]
async fn reporting_a_message_should_send_the_reason_to_the_server() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    let target = ReportTarget::Message { room_id: "!room:example.org".into(), event_id: "$root".into() };
    account.report(&target, "  spam  ").await.unwrap();
    let sent = server.writes.lock().unwrap().clone();
    assert!(sent.iter().any(|(line, body)| line.contains("/report/") && body["reason"] == "spam"), "{sent:?}");
}

#[tokio::test]
async fn reporting_a_person_should_send_the_reason_to_the_server() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    account.report(&ReportTarget::Person { user_id: "@spam:example.org".into() }, "").await.unwrap();
    assert_eq!(server.sent("/report").len(), 1);
}

#[tokio::test]
async fn reporting_a_room_should_send_the_reason_to_the_server() {
    let server = Homeserver::new(json!([]));
    let account = server.synced_account().await;
    account.report(&ReportTarget::Room { room_id: "!other:example.org".into() }, "scam").await.unwrap();
    assert_eq!(server.sent("/report")[0]["reason"], "scam");
}
