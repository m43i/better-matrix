use matrix_sdk::RoomState;
use matrix_sdk::ruma::room::{JoinRuleSummary, RoomType};
use matrix_sdk::ruma::{owned_room_alias_id, owned_room_id};
use serde_json::json;

use super::*;

#[tokio::test]
async fn public_room_discovery_should_include_later_pages_and_exclude_private_rooms_and_spaces() {
    let server = crate::test_server::Server::new(|request| {
        if request.contains("from=second") {
            return json!({"rooms": [{"room_id":"!large:example.org","name":"Large","num_joined_members":80,"world_readable":false,"guest_can_join":false,"join_rule":"public","children_state":[]}]});
        }
        json!({"next_batch":"second","rooms":[
            {"room_id":"!space:example.org","name":"Space","room_type":"m.space","num_joined_members":200,"world_readable":false,"guest_can_join":false,"join_rule":"public","children_state":[
                {"type":"m.space.child","state_key":"!large:example.org","sender":"@a:example.org","origin_server_ts":1,"content":{"via":["z.remote.example.org","a.remote.example.org","z.remote.example.org"]}}
            ]},
            {"room_id":"!small:example.org","name":"Small","num_joined_members":3,"world_readable":false,"guest_can_join":false,"join_rule":"public","children_state":[]},
            {"room_id":"!private:example.org","num_joined_members":300,"world_readable":false,"guest_can_join":false,"join_rule":"invite","children_state":[]}
        ]})
    });
    let account = Account::for_test(&server.url()).await;
    let rooms = account.public_rooms("!space:example.org").await.expect("public rooms");
    assert_eq!(
        rooms.iter().map(|room| (room.room_id.as_str(), room.via.as_slice())).collect::<Vec<_>>(),
        [
            ("!large:example.org", &["z.remote.example.org".to_owned(), "a.remote.example.org".to_owned()][..]),
            ("!small:example.org", &["example.org".to_owned()][..]),
        ]
    );
}

#[tokio::test]
async fn a_repeated_hierarchy_token_should_fail_instead_of_looping() {
    let server = crate::test_server::Server::new(|_| json!({"next_batch":"same","rooms":[]}));
    let account = Account::for_test(&server.url()).await;
    assert!(matches!(account.public_rooms("!space:example.org").await, Err(Error::RepeatedPaginationToken { .. })));
}

// Room previews (permalinks).

fn sdk_preview() -> matrix_sdk::room_preview::RoomPreview {
    matrix_sdk::room_preview::RoomPreview {
        room_id: owned_room_id!("!rust:matrix.org"),
        canonical_alias: Some(owned_room_alias_id!("#rust:matrix.org")),
        name: Some("Rust".into()),
        topic: Some("The language".into()),
        avatar_url: None,
        num_joined_members: 1200,
        num_active_members: None,
        room_type: None,
        join_rule: Some(JoinRuleSummary::Public),
        is_world_readable: Some(true),
        state: None,
        is_direct: None,
        heroes: None,
    }
}

#[test]
fn a_public_room_the_account_never_joined_should_preview_as_joinable() {
    let preview = preview_of(sdk_preview());
    assert_eq!(
        (preview.room_id.as_str(), preview.alias.as_deref(), preview.member_count, preview.joined),
        ("!rust:matrix.org", Some("#rust:matrix.org"), 1200, false)
    );
    assert!(preview.joinable());
}

#[test]
fn a_joined_room_should_preview_as_joined() {
    let preview = preview_of(matrix_sdk::room_preview::RoomPreview { state: Some(RoomState::Joined), ..sdk_preview() });
    assert!(preview.joined && !preview.invited);
}

#[test]
fn an_invite_only_room_should_be_joinable_only_with_an_invite() {
    let rule = Some(JoinRuleSummary::Invite);
    let closed = preview_of(matrix_sdk::room_preview::RoomPreview { join_rule: rule.clone(), ..sdk_preview() });
    let invited = preview_of(matrix_sdk::room_preview::RoomPreview {
        join_rule: rule,
        state: Some(RoomState::Invited),
        ..sdk_preview()
    });
    assert_eq!((closed.join_rule, closed.joinable(), invited.joinable()), (Some(JoinRule::Invite), false, true));
}

#[test]
fn a_room_that_takes_members_who_ask_should_not_offer_to_join() {
    let preview =
        preview_of(matrix_sdk::room_preview::RoomPreview { join_rule: Some(JoinRuleSummary::Knock), ..sdk_preview() });
    assert!(!preview.joinable());
}

#[test]
fn a_space_should_preview_as_a_space() {
    let preview =
        preview_of(matrix_sdk::room_preview::RoomPreview { room_type: Some(RoomType::Space), ..sdk_preview() });
    assert!(preview.space);
}

#[test]
fn a_blank_name_should_preview_as_none() {
    let preview = preview_of(matrix_sdk::room_preview::RoomPreview { name: Some("  ".into()), ..sdk_preview() });
    assert_eq!(preview.name, None);
}

#[test]
fn via_entries_that_are_not_server_names_should_be_left_out() {
    let via = servers(&["matrix.org".into(), "not a server".into(), "example.org:8448".into()]);
    assert_eq!(via.iter().map(|server| server.as_str()).collect::<Vec<_>>(), ["matrix.org", "example.org:8448"]);
}
