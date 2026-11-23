use std::sync::{Arc, Mutex};

use serde_json::json;

use super::*;

fn place(room_id: &str, order: Option<&str>, sent_ms: u64) -> ChildPlace {
    ChildPlace { room_id: room_id.to_owned(), order: order.map(ToOwned::to_owned), sent_ms, suggested: false }
}

fn ids(children: &[ChildPlace]) -> Vec<&str> {
    children.iter().map(|child| child.room_id.as_str()).collect()
}

#[test]
fn children_with_an_order_should_come_first_by_it_and_the_rest_by_when_they_were_added() {
    let mut children =
        [place("!c", None, 1), place("!b", Some("b"), 9), place("!d", None, 0), place("!a", Some("a"), 5)];
    sort_children(&mut children);
    assert_eq!(ids(&children), ["!a", "!b", "!d", "!c"]);
}

#[test]
fn children_with_the_same_order_should_go_by_when_they_were_added() {
    let mut children = [place("!a", Some("x"), 9), place("!b", Some("x"), 1)];
    sort_children(&mut children);
    assert_eq!(ids(&children), ["!b", "!a"]);
}

#[test]
fn the_access_permission_should_need_the_highest_of_its_levels() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    power.events.insert(TimelineEventType::from(StateEventType::RoomJoinRules), Int::from(50u8));
    power.events.insert(TimelineEventType::from(StateEventType::RoomHistoryVisibility), Int::from(100u8));
    assert_eq!(Permission::Access.level(&power), 100);
}

#[test]
fn sending_messages_should_follow_a_messages_own_level() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    let message = TimelineEventType::from(matrix_sdk::ruma::events::MessageLikeEventType::RoomMessage);
    power.events.insert(message.clone(), Int::from(50u8));
    assert_eq!(Permission::SendMessages.level(&power), 50);
    Permission::SendMessages.set(&mut power, Int::from(50u8));
    // The default (polls, reactions) and the room's own message level move together.
    assert_eq!((power.events.get(&message), power.events_default), (Some(&Int::from(50u8)), Int::from(50u8)));
    assert!(Permission::SendMessages.is_set(&power, 50));
    let encrypted = TimelineEventType::from(matrix_sdk::ruma::events::MessageLikeEventType::RoomEncrypted);
    assert!(!power.events.contains_key(&encrypted), "no level of its own made up");
}

#[test]
fn sending_messages_should_need_the_default_where_encrypted_messages_set_no_level() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    power.events_default = Int::from(50u8);
    power
        .events
        .insert(TimelineEventType::from(matrix_sdk::ruma::events::MessageLikeEventType::RoomMessage), Int::from(0u8));
    assert_eq!(Permission::SendMessages.level(&power), 50);
}

#[test]
fn a_permission_should_count_as_set_only_once_every_level_it_writes_is() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    power.events.insert(TimelineEventType::from(StateEventType::RoomJoinRules), Int::from(50u8));
    power.events.insert(TimelineEventType::from(StateEventType::RoomHistoryVisibility), Int::from(0u8));
    power.events.insert(TimelineEventType::from(StateEventType::RoomGuestAccess), Int::from(0u8));
    assert!(!Permission::Access.is_set(&power, 50), "only the highest is 50");
    Permission::Access.set(&mut power, Int::from(50u8));
    assert!(Permission::Access.is_set(&power, 50));
}

#[test]
fn sending_messages_should_need_the_event_default_to_be_changed() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    power.events_default = Int::from(100u8);
    for event in MESSAGE_EVENTS {
        power.events.insert(TimelineEventType::from(event), Int::from(0u8));
    }
    // Members may send messages, but changing it writes the default, which takes an admin.
    assert_eq!((Permission::SendMessages.level(&power), Permission::SendMessages.highest(&power)), (0, 100));
}

#[test]
fn asking_and_space_members_at_once_should_not_read_as_space_members() {
    let both = SdkJoinRule::KnockRestricted(Restricted::new(Vec::new()));
    assert_eq!(join_rule_of(&both), None);
    assert_eq!(join_rule_of(&SdkJoinRule::Restricted(Restricted::new(Vec::new()))), Some(JoinRule::Restricted));
}

#[test]
fn a_space_should_offer_no_upgrade() {
    assert!(!Permission::listed(true).contains(&Permission::Upgrade));
}

#[test]
fn an_order_that_is_too_long_or_unprintable_should_be_ignored() {
    assert_eq!(valid_order(Some(&"a".repeat(51))), None);
    assert_eq!(valid_order(Some("a\u{7f}")), None);
    assert_eq!(valid_order(Some("a b")), Some("a b".to_owned()));
}

#[test]
fn reordering_should_write_only_the_children_out_of_place() {
    let current = [place("!a", Some("000010"), 0), place("!b", Some("000020"), 0), place("!c", None, 0)];
    let wanted = ["!a".to_owned(), "!c".to_owned(), "!b".to_owned()];
    assert_eq!(
        reorder(&current, &wanted),
        [("!c".to_owned(), "000020".to_owned()), ("!b".to_owned(), "000030".to_owned())]
    );
}

#[test]
fn an_upgraded_room_without_an_order_should_keep_its_place() {
    let children = vec![place("!a", None, 1), place("!old", None, 2), place("!b", None, 3)];
    assert_eq!(
        successor_keys(children, "!old", "!new"),
        [("!a", "000010"), ("!new", "000020"), ("!b", "000030")].map(|(id, key)| (id.to_owned(), key.to_owned()))
    );
}

#[test]
fn an_upgraded_room_with_its_own_order_should_need_no_new_keys() {
    let children = vec![place("!a", Some("a"), 1), place("!old", Some("b"), 2)];
    assert!(successor_keys(children, "!old", "!new").is_empty());
}

#[test]
fn a_typed_address_without_a_server_should_be_on_the_accounts_server() {
    assert_eq!(address_of(" rust ", "example.org").unwrap().as_str(), "#rust:example.org");
    assert_eq!(address_of("#rust", "example.org").unwrap().as_str(), "#rust:example.org");
    assert_eq!(address_of("#rust:other.org", "example.org").unwrap().as_str(), "#rust:other.org");
}

#[test]
fn a_new_rooms_address_should_be_its_local_part() {
    assert_eq!(local_part("#rust:example.org").as_deref(), Some("rust"));
    assert_eq!(local_part(" rust ").as_deref(), Some("rust"));
    assert_eq!(local_part(" # "), None);
}

#[test]
fn only_a_higher_numbered_version_should_count_as_newer() {
    assert!(newer("11", "9"));
    assert!(!newer("10", "10"));
    assert!(!newer("org.example.custom", "1"));
}

#[test]
fn a_permission_should_read_back_the_level_it_was_set_to() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    for &permission in Permission::listed(false).iter().chain(Permission::listed(true)) {
        permission.set(&mut power, Int::from(42u8));
        assert_eq!(permission.level(&power), 42, "{permission:?}");
    }
}

#[test]
fn the_access_permission_should_cover_join_rules_history_and_guests() {
    let mut power = RoomPowerLevels::new(
        matrix_sdk::ruma::events::room::power_levels::RoomPowerLevelsSource::None,
        &matrix_sdk::ruma::room_version_rules::AuthorizationRules::V1,
        [],
    );
    Permission::Access.set(&mut power, Int::from(75u8));
    for event in [StateEventType::RoomJoinRules, StateEventType::RoomHistoryVisibility, StateEventType::RoomGuestAccess]
    {
        assert_eq!(power.events.get(&TimelineEventType::from(event)), Some(&Int::from(75u8)));
    }
}

#[test]
fn a_space_should_list_its_rooms_permission_and_a_room_its_messages() {
    assert!(Permission::listed(true).contains(&Permission::SpaceRooms));
    assert!(!Permission::listed(true).contains(&Permission::SendMessages));
    assert!(Permission::listed(false).contains(&Permission::SendMessages));
    assert!(!Permission::listed(false).contains(&Permission::SpaceRooms));
}

#[test]
fn no_rights_should_offer_no_settings() {
    assert!(!RoomRights::default().any());
    assert!(RoomRights { topic: true, ..RoomRights::default() }.any());
}

/// A server that records the room-creation request it gets.
fn creation_server() -> (crate::test_server::Server, Arc<Mutex<Option<serde_json::Value>>>) {
    let body = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&body);
    let server = crate::test_server::Server::new(move |request| {
        if request.contains("/createRoom") {
            let sent = request.split("\r\n\r\n").nth(1).unwrap_or_default();
            *seen.lock().unwrap() = serde_json::from_str(sent).ok();
            return json!({"room_id": "!new:example.org"});
        }
        json!({})
    });
    (server, body)
}

#[tokio::test]
async fn a_public_room_should_be_made_with_its_address_and_listed_in_the_directory() {
    let (server, body) = creation_server();
    let account = Account::for_test(&server.url()).await;
    let new = NewRoom {
        join_rule: JoinRule::Public,
        address: Some("#rust:example.org".into()),
        published: true,
        encrypted: false,
        ..NewRoom::private("Rust", false)
    };
    assert_eq!(account.create_room(&new).await.unwrap(), "!new:example.org");
    let body = body.lock().unwrap().clone().unwrap();
    assert_eq!(body["preset"], "public_chat");
    assert_eq!(body["room_alias_name"], "rust");
    assert_eq!(body["visibility"], "public");
    assert!(body.get("initial_state").is_none_or(|state| state == &json!([])));
}

#[tokio::test]
async fn a_private_room_should_be_made_encrypted_and_unlisted() {
    let (server, body) = creation_server();
    let account = Account::for_test(&server.url()).await;
    account.create_room(&NewRoom::private("Team", false)).await.unwrap();
    let body = body.lock().unwrap().clone().unwrap();
    assert_eq!(body["preset"], "private_chat");
    assert!(body.get("visibility").is_none_or(|v| v == "private"));
    assert_eq!(body["initial_state"][0]["type"], "m.room.encryption");
}

#[tokio::test]
async fn a_room_asking_to_join_should_be_made_with_the_knock_rule() {
    let (server, body) = creation_server();
    let account = Account::for_test(&server.url()).await;
    account.create_room(&NewRoom { join_rule: JoinRule::Knock, ..NewRoom::private("Club", false) }).await.unwrap();
    let body = body.lock().unwrap().clone().unwrap();
    assert_eq!(
        body["initial_state"][0],
        json!({"type":"m.room.join_rules","state_key":"","content":{"join_rule":"knock"}})
    );
}

#[tokio::test]
async fn a_public_space_should_be_made_as_a_space_without_encryption() {
    let (server, body) = creation_server();
    let account = Account::for_test(&server.url()).await;
    let new = NewRoom { join_rule: JoinRule::Public, encrypted: true, ..NewRoom::private("Hub", true) };
    account.create_room(&new).await.unwrap();
    let body = body.lock().unwrap().clone().unwrap();
    assert_eq!(body["creation_content"]["type"], "m.space");
    assert_eq!(body["preset"], "public_chat");
    assert!(body.get("initial_state").is_none_or(|state| state == &json!([])));
}

#[tokio::test]
async fn letting_space_members_join_should_need_a_space() {
    let (server, _) = creation_server();
    let account = Account::for_test(&server.url()).await;
    let new = NewRoom { join_rule: JoinRule::Restricted, ..NewRoom::private("Team", false) };
    assert!(matches!(account.create_room(&new).await, Err(Error::NoParentSpace)));
}

/// A server with `!room:example.org` joined, whose guest-access state lets guests in.
fn guest_server() -> crate::test_server::Server {
    crate::test_server::Server::new(|request| {
        if request.contains("/state/m.room.guest_access") {
            return json!({"guest_access": "can_join"});
        }
        crate::test_server::thread_response(request)
    })
}

#[tokio::test]
async fn guest_access_should_be_read_from_the_server_when_sync_left_it_out() {
    use matrix_sdk::config::SyncSettings;
    let server = guest_server();
    let account = Account::for_test(&server.url()).await;
    account.client().sync_once(SyncSettings::default().timeout(std::time::Duration::ZERO)).await.unwrap();
    let room = account.room("!room:example.org").unwrap();
    assert_eq!(room.guest_access(), GuestAccess::Forbidden, "the store lacks it");
    assert_eq!(account.guest_access(&room).await, Some(true));
}

fn every_right() -> RoomRights {
    RoomRights {
        name: true,
        topic: true,
        avatar: true,
        join_rule: true,
        history: true,
        guest_access: true,
        addresses: true,
        encryption: true,
        permissions: true,
        upgrade: true,
        space_rooms: true,
    }
}

#[test]
fn a_direct_chat_should_offer_no_room_only_settings() {
    let rights = every_right().direct_chat(false);
    assert!(!rights.join_rule && !rights.history && !rights.guest_access);
    assert!(!rights.addresses && !rights.permissions && !rights.upgrade && !rights.space_rooms);
}

#[test]
fn a_group_direct_chat_should_keep_its_name_topic_picture_and_encryption() {
    let rights = every_right().direct_chat(false);
    assert!(rights.name && rights.topic && rights.avatar && rights.encryption);
}

#[test]
fn a_one_to_one_chat_should_offer_only_its_topic_and_encryption() {
    let rights = every_right().direct_chat(true);
    assert_eq!(rights, RoomRights { topic: true, encryption: true, ..RoomRights::default() });
}
