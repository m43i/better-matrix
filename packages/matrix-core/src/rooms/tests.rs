use super::*;
use serde_json::json;

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
