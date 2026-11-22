//! SDK types to [`model`](crate::model) values. The only place that knows both.

use matrix_sdk::deserialized_responses::TimelineEvent;
use matrix_sdk::deserialized_responses::{RawAnySyncOrStrippedTimelineEvent, SyncOrStrippedState};
use matrix_sdk::media::{MediaFormat, MediaThumbnailSettings};
use matrix_sdk::notification_settings::{IsEncrypted, IsOneToOne, NotificationSettings, RoomNotificationMode};
use matrix_sdk::ruma::UInt;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::room::message::{
    AudioMessageEventContent, FormattedBody, MessageFormat, MessageType, SyncRoomMessageEvent,
};
use matrix_sdk::ruma::events::room::power_levels::RoomPowerLevels;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::ruma::events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent, StateEventType};
use matrix_sdk::{Room, RoomMemberships};

use crate::Error;
use crate::model::{
    AccountId, Attachment, AttachmentKind, AudioDetails, HistoryVisibility, MediaRef, MediaSize, Message,
    MessageContent, Notification, NotificationMode, RoomKind, RoomSummary, SpaceSummary,
};

async fn display_name(room: &Room) -> String {
    match room.display_name().await {
        Ok(name) => name.to_string(),
        Err(_) => room.room_id().to_string(),
    }
}

/// A profile or room picture as media to fetch.
pub(crate) fn avatar(url: matrix_sdk::ruma::OwnedMxcUri) -> Option<MediaRef> {
    MediaRef::of(&MediaSource::Plain(url))
}

/// The picture of the person a direct chat is with, from the room's stored members (no request).
async fn partner_avatar(room: &Room, partner: Option<&String>) -> Option<MediaRef> {
    let partner = matrix_sdk::ruma::UserId::parse(partner?).ok()?;
    let member = room.get_member_no_sync(&partner).await.ok()??;
    member.avatar_url().map(ToOwned::to_owned).and_then(avatar)
}

/// A room as the room list shows it. `settings` are the account's push rules, read once for the
/// whole list: reading them per room cost two store reads each, on every sync.
pub(crate) async fn room_summary(room: &Room, settings: &NotificationSettings, rtc_service: bool) -> RoomSummary {
    let name = display_name(room).await;
    let custom = settings.get_user_defined_room_notification_mode(room.room_id()).await;
    let mode = match custom {
        Some(mode) => mode,
        // The cached encryption state: asking for the latest may go to the server.
        None => {
            let encrypted = IsEncrypted::from(room.encryption_state().is_encrypted());
            let one_to_one = IsOneToOne::from(room.active_members_count() == 2);
            settings.get_default_room_notification_mode(encrypted, one_to_one).await
        }
    };
    let kind = if room.is_direct().await.unwrap_or(false) { RoomKind::Direct } else { RoomKind::Group };
    // Read once for every permission the summary shows: each read deserializes the event.
    let power = room.power_levels().await.ok();
    let may_send_state = |event: StateEventType| {
        power.as_ref().is_some_and(|power| power.user_can_send_state(room.own_user_id(), event))
    };
    let dm_users: Vec<String> =
        room.direct_targets().iter().filter_map(|t| t.as_user_id()).map(ToString::to_string).collect();
    let avatar = match room.avatar_url() {
        Some(url) => avatar(url),
        None if kind == RoomKind::Direct => partner_avatar(room, dm_users.first()).await,
        None => None,
    };
    RoomSummary {
        room_id: room.room_id().to_string(),
        name,
        avatar,
        member_count: room.joined_members_count(),
        active_member_count: room.active_members_count(),
        topic: room.topic(),
        kind,
        unread: room.num_unread_messages(),
        mentions: room.num_unread_mentions(),
        dm_users,
        history: history_visibility(room.history_visibility_or_default()),
        notifications: notification_mode(mode),
        custom_notifications: custom.is_some(),
        call: call_in(room, kind, rtc_service, power.as_ref()),
        activity_ms: room.latest_event_timestamp().map(|at| at.get().into()),
        encrypted: room.encryption_state().is_encrypted(),
        public: !private_join_rule(room.join_rule().as_ref()),
        can_rename: may_send_state(StateEventType::RoomName),
        pinned: room.pinned_event_ids().unwrap_or_default().iter().map(ToString::to_string).collect(),
        saved: crate::saved::saved_ids(room).await,
        can_pin: may_send_state(StateEventType::RoomPinnedEvents),
    }
}

/// How a call would start in the room, and who is in its call now.
pub(crate) async fn room_call(room: &Room, kind: RoomKind, rtc_service: bool) -> crate::calls::RoomCall {
    call_in(room, kind, rtc_service, room.power_levels().await.ok().as_ref())
}

/// [`room_call`] with the room's power levels already read.
fn call_in(room: &Room, kind: RoomKind, rtc_service: bool, power: Option<&RoomPowerLevels>) -> crate::calls::RoomCall {
    use matrix_sdk::ruma::events::MessageLikeEventType;
    let me = room.own_user_id();
    let may_join = power.is_some_and(|power| power.user_can_send_state(me, StateEventType::CallMember));
    let may_call = power.is_some_and(|power| power.user_can_send_message(me, MessageLikeEventType::CallInvite));
    let one_to_one = kind == RoomKind::Direct && room.active_members_count() == 2 && may_call;
    crate::calls::RoomCall {
        via: crate::calls::via(rtc_service, may_join, one_to_one),
        participants: room.active_room_call_participants().iter().map(ToString::to_string).collect(),
    }
}

pub(crate) async fn space_summary(room: &Room) -> SpaceSummary {
    // A child event with an empty `via` is a removed child.
    let children = room
        .get_state_events_static::<SpaceChildEventContent>()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|raw| match raw.deserialize().ok()? {
            SyncOrStrippedState::Sync(event) => {
                event.as_original().filter(|e| !e.content.via.is_empty()).map(|e| e.state_key.to_string())
            }
            SyncOrStrippedState::Stripped(_) => None,
        })
        .collect();
    // Only what the store holds: the sync loop loads a space's full member list beside it.
    let members = room
        .members_no_sync(RoomMemberships::JOIN)
        .await
        .unwrap_or_default()
        .iter()
        .map(|member| member.user_id().to_string())
        .collect();
    let power = room.power_levels().await.ok();
    let me = room.own_user_id();
    let may = |event: matrix_sdk::ruma::events::StateEventType| {
        power.as_ref().is_some_and(|power| power.user_can_send_state(me, event))
    };
    SpaceSummary {
        room_id: room.room_id().to_string(),
        name: display_name(room).await,
        avatar: room.avatar_url().and_then(avatar),
        can_add_rooms: may(matrix_sdk::ruma::events::StateEventType::SpaceChild),
        can_rename: may(matrix_sdk::ruma::events::StateEventType::RoomName),
        children,
        members,
    }
}

/// The messages among a room's new timeline events; state and other events are skipped until
/// the timeline renders them.
pub(crate) async fn messages(room: &Room, events: &[TimelineEvent]) -> Vec<Message> {
    let mut messages = Vec::new();
    for event in events {
        if let Some(message) = message(room, event).await {
            messages.push(message);
        }
    }
    messages
}

/// The last of [`messages`], converting only from the newest event back to it.
pub(crate) async fn latest_message(room: &Room, events: &[TimelineEvent]) -> Option<Message> {
    for event in events.iter().rev() {
        if let Some(message) = message(room, event).await {
            return Some(message);
        }
    }
    None
}

/// The event as a message, if it is one ([`messages`]).
async fn message(room: &Room, event: &TimelineEvent) -> Option<Message> {
    let Ok(AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(SyncRoomMessageEvent::Original(
        event,
    )))) = event.raw().deserialize()
    else {
        return None;
    };
    Some(Message {
        event_id: event.event_id.to_string(),
        sender_id: event.sender.to_string(),
        sender_name: sender_name(room, &event.sender).await,
        content: content(&event.content.msgtype),
        timestamp_ms: u64::from(event.origin_server_ts.0),
        ..Message::default()
    })
}

async fn sender_name(room: &Room, sender: &matrix_sdk::ruma::UserId) -> String {
    match room.get_member_no_sync(sender).await {
        Ok(Some(member)) => member.name().to_owned(),
        _ => sender.to_string(),
    }
}

/// A push notification for a room message, if it is one the user should hear about.
/// Whether a join rule keeps strangers out, as MSC4278 lists private rooms: invite, knock,
/// restricted and knock-restricted. Anything else, public, rules this app doesn't know and a room
/// whose rule hasn't loaded, counts as public, so media there waits for a press.
fn private_join_rule(rule: Option<&matrix_sdk::ruma::events::room::join_rules::JoinRule>) -> bool {
    use matrix_sdk::ruma::events::room::join_rules::JoinRule;
    matches!(rule, Some(JoinRule::Invite | JoinRule::Knock | JoinRule::Restricted(_) | JoinRule::KnockRestricted(_)))
}

pub(crate) async fn notification(
    account: &AccountId,
    room: &Room,
    notification: &matrix_sdk::sync::Notification,
) -> Option<Notification> {
    if !notification.actions.iter().any(|action| action.should_notify()) {
        return None;
    }
    let RawAnySyncOrStrippedTimelineEvent::Sync(raw) = &notification.event else { return None };
    let Ok(AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(SyncRoomMessageEvent::Original(
        event,
    )))) = raw.deserialize()
    else {
        return None;
    };
    Some(Notification {
        account: account.clone(),
        room_id: room.room_id().to_string(),
        room_name: display_name(room).await,
        event_id: event.event_id.to_string(),
        sender_name: sender_name(room, &event.sender).await,
        body: content(&event.content.msgtype).plain().to_owned(),
    })
}

impl MediaRef {
    pub(crate) fn of(source: &MediaSource) -> Option<Self> {
        serde_json::to_string(source).ok().map(Self)
    }
}

pub(crate) fn media_source(media: &MediaRef) -> Result<MediaSource, Error> {
    serde_json::from_str(&media.0).map_err(|e| Error::Storage(e.into()))
}

pub(crate) fn media_format(size: MediaSize) -> MediaFormat {
    match size {
        MediaSize::Original => MediaFormat::File,
        MediaSize::Thumbnail { width, height } => {
            MediaFormat::Thumbnail(MediaThumbnailSettings::new(UInt::from(width), UInt::from(height)))
        }
    }
}

pub(crate) fn history_visibility(
    visibility: matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility,
) -> HistoryVisibility {
    use matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility as Sdk;
    match visibility {
        Sdk::WorldReadable => HistoryVisibility::WorldReadable,
        Sdk::Invited => HistoryVisibility::Invited,
        Sdk::Joined => HistoryVisibility::Joined,
        _ => HistoryVisibility::Shared,
    }
}

pub(crate) fn notification_mode(mode: RoomNotificationMode) -> NotificationMode {
    match mode {
        RoomNotificationMode::AllMessages => NotificationMode::AllMessages,
        RoomNotificationMode::MentionsAndKeywordsOnly => NotificationMode::MentionsOnly,
        RoomNotificationMode::Mute => NotificationMode::Mute,
    }
}

pub(crate) fn room_notification_mode(mode: NotificationMode) -> RoomNotificationMode {
    match mode {
        NotificationMode::AllMessages => RoomNotificationMode::AllMessages,
        NotificationMode::MentionsOnly => RoomNotificationMode::MentionsAndKeywordsOnly,
        NotificationMode::Mute => RoomNotificationMode::Mute,
    }
}

/// A sticker: an image like any other.
pub(crate) fn sticker(sticker: &matrix_sdk::ruma::events::sticker::StickerEventContent) -> MessageContent {
    let info = &sticker.info;
    let dimensions = || Some((u32::try_from(info.width?).ok()?, u32::try_from(info.height?).ok()?));
    MessageContent::Attachment(Attachment {
        kind: AttachmentKind::Image,
        name: sticker.body.clone(),
        size: info.size.map(u64::from),
        media: MediaRef::of(&MediaSource::from(sticker.source.clone())),
        thumbnail: None,
        dimensions: dimensions(),
        audio: None,
    })
}

pub(crate) fn content(msgtype: &MessageType) -> MessageContent {
    let text = |body: &str, formatted: &Option<FormattedBody>| MessageContent::Text {
        body: body.to_owned(),
        html: formatted.as_ref().filter(|f| f.format == MessageFormat::Html).map(|f| f.body.clone()),
    };
    let attachment = |kind, name: &str, source: &MediaSource, size: Option<UInt>| Attachment {
        kind,
        name: name.to_owned(),
        size: size.map(u64::from),
        media: MediaRef::of(source),
        thumbnail: None,
        dimensions: None,
        audio: None,
    };
    let dimensions = |w: Option<UInt>, h: Option<UInt>| Some((u32::try_from(w?).ok()?, u32::try_from(h?).ok()?));
    let thumbnail = |source: Option<&MediaSource>| source.and_then(MediaRef::of);
    let attachment = match msgtype {
        MessageType::Text(c) => return text(&c.body, &c.formatted),
        MessageType::Notice(c) => return text(&c.body, &c.formatted),
        MessageType::Emote(c) => return text(&format!("* {}", c.body), &None),
        MessageType::Image(c) => {
            let info = c.info.as_deref();
            Attachment {
                thumbnail: thumbnail(info.and_then(|i| i.thumbnail_source.as_ref())),
                dimensions: info.and_then(|i| dimensions(i.width, i.height)),
                ..attachment(AttachmentKind::Image, c.filename(), &c.source, info.and_then(|i| i.size))
            }
        }
        MessageType::Video(c) => {
            let info = c.info.as_deref();
            Attachment {
                thumbnail: thumbnail(info.and_then(|i| i.thumbnail_source.as_ref())),
                dimensions: info.and_then(|i| dimensions(i.width, i.height)),
                ..attachment(AttachmentKind::Video, c.filename(), &c.source, info.and_then(|i| i.size))
            }
        }
        MessageType::Audio(c) => Attachment {
            audio: audio_details(c),
            ..attachment(AttachmentKind::Audio, c.filename(), &c.source, c.info.as_deref().and_then(|i| i.size))
        },
        MessageType::File(c) => {
            attachment(AttachmentKind::File, c.filename(), &c.source, c.info.as_deref().and_then(|i| i.size))
        }
        other => return MessageContent::Text { body: other.body().to_owned(), html: None },
    };
    MessageContent::Attachment(attachment)
}

/// What an audio message says about itself: MSC3245's voice block marks a voice memo, MSC1767's
/// audio block carries the waveform (and a length, which `info` may carry instead).
fn audio_details(c: &AudioMessageEventContent) -> Option<AudioDetails> {
    let duration = c.audio.as_ref().map(|a| a.duration).or_else(|| c.info.as_deref().and_then(|i| i.duration));
    let waveform: Vec<u16> = c.audio.as_ref().map_or_else(Vec::new, |a| {
        a.waveform.iter().map(|v| u16::try_from(v.get()).unwrap_or(u16::MAX).min(AudioDetails::WAVEFORM_MAX)).collect()
    });
    let voice = c.voice.is_some();
    (voice || duration.is_some() || !waveform.is_empty()).then(|| AudioDetails {
        voice,
        duration_ms: duration.map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        waveform,
    })
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::events::room::ImageInfo;
    use matrix_sdk::ruma::events::sticker::StickerEventContent;
    use matrix_sdk::ruma::{OwnedMxcUri, uint};

    use super::*;

    #[test]
    fn invite_knock_and_restricted_join_rules_should_count_as_private() {
        use matrix_sdk::ruma::events::room::join_rules::{JoinRule, Restricted};
        assert!(private_join_rule(Some(&JoinRule::Invite)));
        assert!(private_join_rule(Some(&JoinRule::Knock)));
        assert!(private_join_rule(Some(&JoinRule::Restricted(Restricted::new(Vec::new())))));
        assert!(private_join_rule(Some(&JoinRule::KnockRestricted(Restricted::new(Vec::new())))));
    }

    #[test]
    fn public_unknown_and_missing_join_rules_should_count_as_public() {
        use matrix_sdk::ruma::events::room::join_rules::JoinRule;
        assert!(!private_join_rule(None));
        assert!(!private_join_rule(Some(&JoinRule::Public)));
        assert!(!private_join_rule(Some(&JoinRule::Private)));
        let custom: JoinRule = serde_json::from_value(serde_json::json!({ "join_rule": "org.example.open" })).unwrap();
        assert!(!private_join_rule(Some(&custom)));
    }

    #[test]
    fn a_sticker_should_show_as_an_image_named_by_its_description() {
        let mut info = ImageInfo::new();
        info.width = Some(uint!(128));
        info.height = Some(uint!(96));
        let url = OwnedMxcUri::from("mxc://example.org/sticker");
        let MessageContent::Attachment(attachment) =
            sticker(&StickerEventContent::new("A waving cat".into(), info, url))
        else {
            panic!("a sticker should be an attachment");
        };
        assert_eq!(
            (attachment.kind, attachment.name.as_str(), attachment.dimensions, attachment.media.is_some()),
            (AttachmentKind::Image, "A waving cat", Some((128, 96)), true)
        );
    }

    #[tokio::test]
    async fn the_latest_message_should_be_the_last_of_the_messages() {
        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = crate::Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync room");
        let room = account.client().get_room(matrix_sdk::ruma::room_id!("!room:example.org")).expect("room");
        let event = |value: serde_json::Value| {
            TimelineEvent::from_plaintext(serde_json::from_value(value).unwrap_or_else(|e| panic!("{e}")))
        };
        let message = |id: &str| {
            event(serde_json::json!({"type":"m.room.message", "event_id":id, "sender":"@b:example.org",
                "origin_server_ts":1, "content":{"msgtype":"m.text", "body":id}}))
        };
        let reaction =
            event(serde_json::json!({"type":"m.reaction", "event_id":"$reaction", "sender":"@b:example.org",
            "origin_server_ts":2, "content":{"m.relates_to":{"rel_type":"m.annotation", "event_id":"$a", "key":"x"}}}));
        let events = [message("$a"), message("$b"), reaction];
        let latest = latest_message(&room, &events).await;
        assert!(latest.is_some() && latest == messages(&room, &events).await.pop(), "{latest:?}");
    }

    fn audio(json: serde_json::Value) -> Option<AudioDetails> {
        let msgtype: MessageType = serde_json::from_value(json).unwrap_or_else(|e| panic!("{e}"));
        let MessageContent::Attachment(attachment) = content(&msgtype) else { panic!("audio should be an attachment") };
        attachment.audio
    }

    #[test]
    fn a_voice_message_should_carry_its_length_and_waveform() {
        let details = audio(serde_json::json!({
            "msgtype": "m.audio", "body": "Voice message.ogg", "url": "mxc://example.org/voice",
            "org.matrix.msc1767.audio": { "duration": 4200, "waveform": [0, 512, 1024] },
            "org.matrix.msc3245.voice": {},
        }));
        assert_eq!(details, Some(AudioDetails { voice: true, duration_ms: Some(4200), waveform: vec![0, 512, 1024] }));
    }

    #[test]
    fn an_audio_file_without_details_should_have_none() {
        let details = audio(serde_json::json!({
            "msgtype": "m.audio", "body": "song.mp3", "url": "mxc://example.org/song",
        }));
        assert_eq!(details, None);
    }
}
