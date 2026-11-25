//! The user's note to self: a room only they are in, for notes, links and files they keep for
//! themselves. Matrix has no such room of its own, so it is a normal room (private, encrypted,
//! named in the user's language), and the account data event `chat.bettermatrix.note_to_self`
//! names it: every session of this app finds the same one and shows it apart; other clients show
//! a normal room.

use std::time::Duration;

use matrix_sdk::ruma::events::{AnyGlobalAccountDataEvent, GlobalAccountDataEventType};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedRoomId, RoomId};
use matrix_sdk::{Room, RoomState};
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::account::Account;

/// The account data event naming the room.
const EVENT_TYPE: &str = "chat.bettermatrix.note_to_self";

/// How long a new note to self waits for sync to bring its mark, so the room list shows it apart
/// from the start.
const SETTLE: Duration = Duration::from_secs(10);

/// How often the wait looks.
const LOOK_EVERY: Duration = Duration::from_millis(250);

/// The account data event's content.
#[derive(Serialize, Deserialize)]
struct Mark {
    room_id: OwnedRoomId,
}

/// Whether a sync's account data names another room, or none.
pub(crate) fn changed_in(account_data: &[Raw<AnyGlobalAccountDataEvent>]) -> bool {
    account_data.iter().any(|raw| raw.get_field::<String>("type").ok().flatten().as_deref() == Some(EVENT_TYPE))
}

/// The room an event's content names; none when it has another shape.
fn marked_in(content: &str) -> Option<OwnedRoomId> {
    serde_json::from_str::<Mark>(content).ok().map(|mark| mark.room_id)
}

/// Whether the server turned a join away (forbidden, or no such room), rather than failing.
fn refused(error: &matrix_sdk::Error) -> bool {
    error.as_client_api_error().is_some_and(|api| matches!(api.status_code.as_u16(), 403 | 404))
}

/// Whether `room` shows as the note to self: the marked room while nobody else has joined or been
/// invited, nobody may join without an invite, and no upgrade replaced it (one that did is read
/// only). Otherwise it is a room like any other, and the next note to self is a new one.
pub(crate) fn is_note_to_self(room: &Room, marked: Option<&RoomId>) -> bool {
    use matrix_sdk::ruma::events::room::join_rules::JoinRule;
    // No join rule known yet: a room just made, private.
    let private = matches!(room.join_rule(), None | Some(JoinRule::Invite));
    marked == Some(room.room_id()) && room.active_members_count() <= 1 && private && !room.is_tombstoned()
}

impl Account {
    /// The room the account data names, as this device stored it (no request).
    pub(crate) async fn marked_note_to_self(&self) -> Option<OwnedRoomId> {
        let event_type = GlobalAccountDataEventType::from(EVENT_TYPE);
        let raw = self.client().account().account_data_raw(event_type).await.ok()??;
        marked_in(raw.json().get())
    }

    /// The note to self: the marked room while the user is still in it alone, else a new private,
    /// encrypted room named `name`, marked as the note to self. A new one returns once sync has
    /// brought its mark (at most [`SETTLE`]), so the room list already shows it apart.
    pub async fn note_to_self(&self, name: &str) -> Result<String, Error> {
        let account = self.client().account();
        let event_type = GlobalAccountDataEventType::from(EVENT_TYPE);
        // The server's mark: this device's copy may be stale, or missing one another session made.
        let marked = account.fetch_account_data(event_type.clone()).await?.and_then(|raw| marked_in(raw.json().get()));
        if let Some(room_id) = marked {
            // A room another session made may not have synced here yet: joining one the user is
            // in changes nothing, and one they left (invite only) refuses them. Any other failure
            // fails the command: making another would replace the room holding their notes.
            let room = match self.client().get_room(&room_id) {
                Some(room) => Some(room),
                None => match self.client().join_room_by_id(&room_id).await {
                    Ok(room) => Some(room),
                    Err(error) if refused(&error) => None,
                    Err(error) => return Err(error.into()),
                },
            };
            if let Some(room) = room
                && room.state() == RoomState::Joined
                && is_note_to_self(&room, Some(room.room_id()))
            {
                return Ok(room.room_id().to_string());
            }
        }
        let room_id = self.create_room(&crate::NewRoom::private(name, false)).await?;
        let mark = Mark { room_id: RoomId::parse(&room_id)? };
        let raw = Raw::new(&mark).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        account.set_account_data_raw(event_type, raw).await?;
        let marked = async {
            while self.marked_note_to_self().await.as_deref().map(RoomId::as_str) != Some(room_id.as_str()) {
                tokio::time::sleep(LOOK_EVERY).await;
            }
        };
        let _ = tokio::time::timeout(SETTLE, marked).await;
        Ok(room_id)
    }

    /// Ends the note to self once the user invites someone to it: the room is then theirs to
    /// share, never the note to self again, and the next note to self is a new room. Other rooms
    /// are left as they are. The server's mark decides, as this device's may be stale.
    pub(crate) async fn shared_note_to_self(&self, room_id: &RoomId) -> Result<(), Error> {
        let account = self.client().account();
        let event_type = GlobalAccountDataEventType::from(EVENT_TYPE);
        let marked = account.fetch_account_data(event_type.clone()).await?.and_then(|raw| marked_in(raw.json().get()));
        if marked.as_deref() != Some(room_id) {
            return Ok(());
        }
        let cleared = Raw::new(&serde_json::json!({})).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        account.set_account_data_raw(event_type, cleared).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn a_mark_should_name_its_room() {
        assert_eq!(
            marked_in(r#"{"room_id":"!notes:example.org"}"#).as_deref().map(RoomId::as_str),
            Some("!notes:example.org")
        );
    }

    #[test]
    fn a_mark_of_another_shape_should_name_no_room() {
        assert_eq!(marked_in(r#"{"room":"!notes:example.org"}"#), None);
        assert_eq!(marked_in(r#"{"room_id":"not a room"}"#), None);
    }

    #[test]
    fn a_sync_carrying_the_mark_should_count_as_a_change() {
        let event = |kind: &str| Raw::from_json_string(json!({"type":kind,"content":{}}).to_string()).expect("raw");
        assert!(changed_in(&[event("m.direct"), event(EVENT_TYPE)]));
        assert!(!changed_in(&[event("m.direct")]));
    }

    /// What the server recorded: the rooms made, and the mark it holds.
    #[derive(Default)]
    struct Seen {
        made: Vec<Value>,
        mark: Option<String>,
        syncs: u32,
        /// Sync leaves the mark out, as for a device that hasn't synced it yet.
        unsynced: bool,
        joined: Vec<String>,
        /// What a join answers instead of joining: a refusal or a failure.
        join_reply: Option<Value>,
        /// The room's join rule, when sync should carry one.
        join_rule: Option<&'static str>,
        /// An upgrade replaced the room.
        replaced: bool,
        /// Reading the mark is refused.
        unreadable: bool,
    }

    /// A server whose sync carries `!room:example.org` with `members` joined, and the mark it
    /// holds: one set is there for the next sync, as a homeserver's would be.
    fn server(members: u64, seen: Arc<Mutex<Seen>>) -> crate::test_server::Server {
        with_invited(members, 0, seen)
    }

    /// [`server`], with `invited` people invited to the room besides.
    fn with_invited(members: u64, invited: u64, seen: Arc<Mutex<Seen>>) -> crate::test_server::Server {
        crate::test_server::Server::new(move |request| {
            let body =
                || serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap_or(Value::Null);
            let mut seen = seen.lock().expect("seen");
            if request.starts_with("POST") && request.contains("/createRoom") {
                seen.made.push(body());
                return json!({"room_id":"!new:example.org"});
            }
            if request.starts_with("POST") && request.contains("/join") {
                seen.joined.push(request.lines().next().unwrap_or_default().to_owned());
                return seen.join_reply.clone().unwrap_or_else(|| json!({"room_id":seen.mark.clone()}));
            }
            if request.starts_with("POST") && request.contains("/invite") {
                return json!({});
            }
            if request.contains(&format!("/account_data/{EVENT_TYPE}")) {
                if request.starts_with("PUT") {
                    seen.mark = marked_in(&body().to_string()).map(|room_id| room_id.to_string());
                    // A new mark syncs as any would.
                    seen.unsynced = false;
                    return json!({});
                }
                if seen.unreadable {
                    return json!({"errcode":"M_FORBIDDEN","error":"not now"});
                }
                return match &seen.mark {
                    Some(room_id) => json!({"room_id":room_id}),
                    None => json!({"errcode":"M_NOT_FOUND","error":"no mark"}),
                };
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                // A token of its own each time: the SDK skips a response it has seen.
                seen.syncs += 1;
                response["next_batch"] = json!(format!("sync-{}", seen.syncs));
                let summary = &mut response["rooms"]["join"]["!room:example.org"]["summary"];
                summary["m.joined_member_count"] = json!(members);
                summary["m.invited_member_count"] = json!(invited);
                let mut events = Vec::new();
                if let Some(rule) = seen.join_rule {
                    events.push(json!({"type":"m.room.join_rules","state_key":"","sender":"@a:example.org","event_id":"$rule","origin_server_ts":0,"content":{"join_rule":rule}}));
                }
                if seen.replaced {
                    events.push(json!({"type":"m.room.tombstone","state_key":"","sender":"@a:example.org","event_id":"$tombstone","origin_server_ts":0,"content":{"body":"upgraded","replacement_room":"!v2:example.org"}}));
                }
                if let Some(state) = response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut() {
                    state.extend(events);
                }
                if let Some(room_id) = seen.mark.as_ref().filter(|_| !seen.unsynced) {
                    response["account_data"] = json!({"events":[{"type":EVENT_TYPE,"content":{"room_id":room_id}}]});
                }
            }
            response
        })
    }

    fn marked(room_id: &str) -> Arc<Mutex<Seen>> {
        Arc::new(Mutex::new(Seen { mark: Some(room_id.into()), ..Seen::default() }))
    }

    async fn sync(account: &Account) {
        let settings = matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO);
        account.client().sync_once(settings).await.expect("sync");
    }

    async fn synced(server: &crate::test_server::Server) -> Account {
        let account = Account::for_test(&server.url()).await;
        sync(&account).await;
        account
    }

    /// Opens the note to self while syncing goes on, as the engine's sync loop would.
    async fn open(account: &Account, name: &str) -> String {
        let syncing = async {
            loop {
                tokio::time::sleep(LOOK_EVERY).await;
                sync(account).await;
            }
        };
        tokio::select! {
            opened = account.note_to_self(name) => opened.expect("note to self"),
            () = syncing => unreachable!("syncing goes on"),
        }
    }

    #[tokio::test]
    async fn the_note_to_self_should_be_the_marked_room_while_the_user_is_alone_in_it() {
        let seen = marked("!room:example.org");
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        assert_eq!(open(&account, "Note to self").await, "!room:example.org");
        assert!(seen.lock().expect("seen").made.is_empty());
    }

    #[tokio::test]
    async fn a_marked_room_someone_else_joined_should_give_way_to_a_new_note_to_self() {
        let seen = marked("!room:example.org");
        let homeserver = server(2, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        assert_eq!(open(&account, "Note to self").await, "!new:example.org");
        assert_eq!(seen.lock().expect("seen").mark.as_deref(), Some("!new:example.org"));
    }

    #[tokio::test]
    async fn a_note_to_self_this_device_has_not_synced_should_be_joined_rather_than_made_again() {
        let seen = Arc::new(Mutex::new(Seen {
            mark: Some("!elsewhere:example.org".into()),
            unsynced: true,
            ..Seen::default()
        }));
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        assert_eq!(account.note_to_self("Note to self").await.expect("note to self"), "!elsewhere:example.org");
        let seen = seen.lock().expect("seen");
        assert_eq!(seen.joined.len(), 1, "{:?}", seen.joined);
        assert!(seen.made.is_empty());
    }

    /// A mark naming a room this device doesn't know, whose join answers `reply`.
    fn unsynced_with_join(reply: Value) -> Arc<Mutex<Seen>> {
        Arc::new(Mutex::new(Seen {
            mark: Some("!elsewhere:example.org".into()),
            unsynced: true,
            join_reply: Some(reply),
            ..Seen::default()
        }))
    }

    #[tokio::test]
    async fn a_marked_room_the_user_may_not_rejoin_should_give_way_to_a_new_one() {
        let seen = unsynced_with_join(json!({"errcode":"M_FORBIDDEN","error":"not invited"}));
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        assert_eq!(open(&account, "Note to self").await, "!new:example.org");
    }

    #[tokio::test]
    async fn a_join_that_fails_should_fail_rather_than_replace_the_note_to_self() {
        // An answer the client can't read: a failure, not a refusal (the SDK retries a server
        // error until it gives up, too slow for a test).
        let seen = unsynced_with_join(json!({"unexpected":true}));
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        assert!(account.note_to_self("Note to self").await.is_err());
        let seen = seen.lock().expect("seen");
        assert!(seen.made.is_empty());
        assert_eq!(seen.mark.as_deref(), Some("!elsewhere:example.org"));
    }

    #[tokio::test]
    async fn a_marked_room_anyone_may_join_should_not_be_the_note_to_self() {
        let seen = Arc::new(Mutex::new(Seen { join_rule: Some("public"), ..Seen::default() }));
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        let marked = RoomId::parse("!room:example.org").expect("id");
        let room = account.client().get_room(&marked).expect("room");
        assert!(!is_note_to_self(&room, Some(&marked)));
    }

    #[tokio::test]
    async fn an_upgraded_note_to_self_should_give_way_to_a_new_one() {
        let seen =
            Arc::new(Mutex::new(Seen { mark: Some("!room:example.org".into()), replaced: true, ..Seen::default() }));
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        assert_eq!(open(&account, "Note to self").await, "!new:example.org");
    }

    #[tokio::test]
    async fn inviting_into_a_room_this_device_still_thinks_marked_should_keep_the_newer_mark() {
        let seen = marked("!room:example.org");
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        // Another session moved the mark since; this device hasn't synced it.
        {
            let mut seen = seen.lock().expect("seen");
            seen.mark = Some("!moved:example.org".into());
            seen.unsynced = true;
        }
        account.invite("!room:example.org", "@b:example.org").await.expect("invited");
        assert_eq!(seen.lock().expect("seen").mark.as_deref(), Some("!moved:example.org"));
    }

    #[tokio::test]
    async fn the_servers_mark_should_win_over_a_stale_one_here() {
        let seen = marked("!room:example.org");
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        // Another session moved the mark since; this device hasn't synced it.
        {
            let mut seen = seen.lock().expect("seen");
            seen.mark = Some("!moved:example.org".into());
            seen.unsynced = true;
        }
        assert_eq!(account.note_to_self("Note to self").await.expect("note to self"), "!moved:example.org");
        assert!(seen.lock().expect("seen").made.is_empty());
    }

    #[tokio::test]
    async fn inviting_someone_to_the_note_to_self_should_end_it() {
        let seen = marked("!room:example.org");
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        account.invite("!room:example.org", "@b:example.org").await.expect("invited");
        assert_eq!(seen.lock().expect("seen").mark, None);
    }

    #[tokio::test]
    async fn an_invite_should_be_delivered_even_when_the_mark_cannot_be_read() {
        let seen = Arc::new(Mutex::new(Seen { unreadable: true, ..Seen::default() }));
        let homeserver = server(2, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        account.invite("!room:example.org", "@b:example.org").await.expect("invited");
    }

    #[tokio::test]
    async fn inviting_someone_to_another_room_should_keep_the_note_to_self() {
        let seen = marked("!notes:example.org");
        let homeserver = server(2, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        account.invite("!room:example.org", "@b:example.org").await.expect("invited");
        assert_eq!(seen.lock().expect("seen").mark.as_deref(), Some("!notes:example.org"));
    }

    #[tokio::test]
    async fn inviting_someone_should_make_the_note_to_self_a_room_and_the_next_one_new() {
        let seen = marked("!room:example.org");
        let homeserver = with_invited(1, 1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        let room = account.client().get_room(&RoomId::parse("!room:example.org").expect("id")).expect("room");
        assert!(!is_note_to_self(&room, Some(room.room_id())));
        assert_eq!(open(&account, "Note to self").await, "!new:example.org");
    }

    #[tokio::test]
    async fn a_new_note_to_self_should_wait_for_sync_to_bring_its_mark() {
        let homeserver = server(1, Arc::default());
        let account = synced(&homeserver).await;
        open(&account, "Note to self").await;
        assert_eq!(account.marked_note_to_self().await.as_deref().map(RoomId::as_str), Some("!new:example.org"));
    }

    #[tokio::test]
    async fn a_new_note_to_self_should_be_a_private_encrypted_room_with_its_name() {
        let seen = Arc::default();
        let homeserver = server(1, Arc::clone(&seen));
        let account = synced(&homeserver).await;
        open(&account, "Notiz an mich").await;
        let seen = seen.lock().expect("seen");
        let room = &seen.made[0];
        assert_eq!(room["name"], "Notiz an mich");
        assert_eq!(room["preset"], "private_chat");
        assert!(room.get("invite").is_none_or(|invite| invite.as_array().is_some_and(Vec::is_empty)));
        assert!(room["initial_state"].as_array().expect("state").iter().any(|e| e["type"] == "m.room.encryption"));
    }

    #[tokio::test]
    async fn only_the_marked_room_alone_should_show_as_the_note_to_self() {
        let marked = RoomId::parse("!room:example.org").expect("id");
        let other = RoomId::parse("!other:example.org").expect("id");
        let homeserver = server(1, Arc::default());
        let account = synced(&homeserver).await;
        let room = account.client().get_room(&marked).expect("room");
        assert!(is_note_to_self(&room, Some(&marked)));
        assert!(!is_note_to_self(&room, Some(&other)));
        assert!(!is_note_to_self(&room, None));

        let homeserver = server(2, Arc::default());
        let account = synced(&homeserver).await;
        let room = account.client().get_room(&marked).expect("room");
        assert!(!is_note_to_self(&room, Some(&marked)));
    }

    #[tokio::test]
    async fn the_note_to_self_should_be_listed_with_the_direct_chats() {
        let homeserver = server(1, Arc::default());
        let account = synced(&homeserver).await;
        let marked = RoomId::parse("!room:example.org").expect("id");
        let room = account.client().get_room(&marked).expect("room");
        let settings = account.client().notification_settings().await;
        let summary = crate::convert::room_summary(&room, &settings, Some(&marked), true, false, &account.sticky).await;
        assert!(summary.note_to_self);
        assert_eq!(summary.kind, crate::RoomKind::Direct);
        assert!(summary.dm_users.is_empty());
        assert_eq!(summary.call.via, None);
        let unmarked = crate::convert::room_summary(&room, &settings, None, true, false, &account.sticky).await;
        assert!(!unmarked.note_to_self);
        assert_eq!(unmarked.kind, crate::RoomKind::Group);
    }
}
