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
use matrix_sdk::ruma::events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent, StateEventType};
use matrix_sdk::{Room, RoomMemberships};
use matrix_sdk_base::RoomDisplayName;

use crate::Error;
use crate::model::{
    AccountId, Attachment, AttachmentKind, AudioDetails, HistoryVisibility, MediaRef, MediaSize, Message,
    MessageContent, Notification, NotificationMode, RoomKind, RoomPriority, RoomSummary, SpaceSummary,
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

/// A room as the room list shows it. `settings` are the account's push rules and `note_to_self`
/// the room its account data names, each read once for the whole list: reading them per room
/// cost store reads for each, on every sync.
pub(crate) async fn room_summary(
    room: &Room,
    settings: &NotificationSettings,
    note_to_self: Option<&matrix_sdk::ruma::RoomId>,
    _rtc_service: bool,
    meetings: bool,
    _sticky: &crate::calls::StickyMembers,
) -> RoomSummary {
    let display = room.display_name().await.ok();
    let name = display.as_ref().map_or_else(|| room.room_id().to_string(), ToString::to_string);
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
    let direct = room.is_direct().await.unwrap_or(false);
    let note_to_self = crate::note_to_self::is_note_to_self(room, note_to_self);
    // The note to self is listed with the direct chats, though it is a normal room.
    let kind = if direct || note_to_self { RoomKind::Direct } else { RoomKind::Group };
    // Read once for every permission the summary shows: each read deserializes the event.
    let power = room.power_levels().await.ok();
    // What its settings offer: a direct chat's are narrower (`RoomRights::direct_chat`); the note
    // to self keeps a room's, so it can be renamed and given a picture.
    let rights = power.as_ref().map_or_else(crate::RoomRights::default, |power| {
        let rights = crate::RoomRights::of(power, room.own_user_id(), room.encryption_state().is_encrypted(), false);
        if direct { rights.direct_chat(room.active_members_count() <= 2) } else { rights }
    });
    let may_send_state = |event: StateEventType| {
        power.as_ref().is_some_and(|power| power.user_can_send_state(room.own_user_id(), event))
    };
    // The note to self is with nobody, whatever another client's `m.direct` says.
    let dm_users: Vec<String> = if note_to_self {
        Vec::new()
    } else {
        room.direct_targets().iter().filter_map(|t| t.as_user_id()).map(ToString::to_string).collect()
    };
    let name_from_member = direct
        && dm_users.len() == 1
        && room.active_members_count() <= 2
        && matches!(display, Some(RoomDisplayName::Calculated(_)));
    let avatar = match room.avatar_url() {
        Some(url) => avatar(url),
        None if note_to_self => partner_avatar(room, Some(&room.own_user_id().to_string())).await,
        None if kind == RoomKind::Direct => partner_avatar(room, dm_users.first()).await,
        None => None,
    };
    #[cfg(feature = "native-media")]
    let call = {
        let mut call =
            call_in(room, kind, _rtc_service, power.as_ref(), crate::calls::joined_sticky(room, _sticky, false).await);
        if note_to_self {
            call.via = None;
        }
        call
    };
    #[cfg(not(feature = "native-media"))]
    let call = crate::calls::RoomCall::default();
    let latest = room.latest_event();
    RoomSummary {
        room_id: room.room_id().to_string(),
        name,
        name_from_member,
        avatar,
        member_count: room.joined_members_count(),
        active_member_count: room.active_members_count(),
        topic: room.topic(),
        kind,
        unread: room.num_unread_messages(),
        mentions: room.num_unread_mentions(),
        dm_users,
        note_to_self,
        history: history_visibility(room.history_visibility_or_default()),
        notifications: notification_mode(mode),
        custom_notifications: custom.is_some(),
        call,
        activity_ms: latest.timestamp().map(|at| at.get().into()),
        latest_event_id: latest.event_id().map(|id| id.to_string()),
        encrypted: room.encryption_state().is_encrypted(),
        public: !private_join_rule(room.join_rule().as_ref()),
        can_rename: rights.name,
        pinned: room.pinned_event_ids().unwrap_or_default().iter().map(ToString::to_string).collect(),
        saved: crate::saved::saved_ids(room).await,
        can_share_live_location: power.as_ref().is_some_and(|power| {
            crate::geo::live::permitted(power, room.own_user_id(), room.encryption_state().is_encrypted())
        }),
        can_pin: may_send_state(StateEventType::RoomPinnedEvents),
        // Where the homeserver names a Jitsi server and the user may pin widgets.
        can_start_meeting: meetings
            && !note_to_self
            && may_send_state(StateEventType::from(crate::widgets::LEGACY_TYPE)),
        priority: if room.is_favourite() {
            RoomPriority::Favourite
        } else if room.is_low_priority() {
            RoomPriority::Low
        } else {
            RoomPriority::Normal
        },
        marked_unread: room.is_marked_unread(),
        can_change_settings: rights.any(),
        knocks: knocks(room, power.as_ref()).await,
        successor: successor(room).await,
    }
}

/// How many people ask to join, when the room lets people ask and the user may let them in.
async fn knocks(room: &Room, power: Option<&RoomPowerLevels>) -> u64 {
    use matrix_sdk::ruma::room::JoinRule;
    let asks = matches!(room.join_rule(), Some(JoinRule::Knock | JoinRule::KnockRestricted(_)));
    if !asks || !power.is_some_and(|power| power.user_can_invite(room.own_user_id())) {
        return 0;
    }
    room.members_no_sync(RoomMemberships::KNOCK).await.map_or(0, |knocking| knocking.len() as u64)
}

/// The room that replaced `room`, with servers to join it through: the upgrader's, the old
/// room's and the account's own.
async fn successor(room: &Room) -> Option<crate::Successor> {
    use matrix_sdk::ruma::events::room::tombstone::RoomTombstoneEventContent;
    let successor = room.successor_room()?;
    let mut via: Vec<String> = Vec::new();
    if let Ok(Some(raw)) = room.get_state_event_static::<RoomTombstoneEventContent>().await
        && let Ok(SyncOrStrippedState::Sync(event)) = raw.deserialize()
    {
        via.push(event.sender().server_name().to_string());
    }
    via.extend(room.room_id().server_name().map(ToString::to_string));
    via.push(room.own_user_id().server_name().to_string());
    let mut seen = std::collections::HashSet::new();
    via.retain(|server| seen.insert(server.clone()));
    Some(crate::Successor { room_id: successor.room_id.to_string(), reason: successor.reason, via })
}

/// How a call would start in the room, and who is in its call now.
#[cfg(feature = "native-media")]
pub(crate) async fn room_call(
    room: &Room,
    kind: RoomKind,
    rtc_service: bool,
    sticky: &crate::calls::StickyMembers,
) -> crate::calls::RoomCall {
    let sticky = crate::calls::joined_sticky(room, sticky, false).await;
    call_in(room, kind, rtc_service, room.power_levels().await.ok().as_ref(), sticky)
}

/// [`room_call`] with the room's power levels already read.
#[cfg(feature = "native-media")]
fn call_in(
    room: &Room,
    kind: RoomKind,
    rtc_service: bool,
    power: Option<&RoomPowerLevels>,
    sticky: Vec<crate::calls::Member>,
) -> crate::calls::RoomCall {
    use matrix_sdk::ruma::events::MessageLikeEventType;
    let me = room.own_user_id();
    let may_join = power.is_some_and(|power| power.user_can_send_state(me, StateEventType::CallMember));
    let may_call = power.is_some_and(|power| power.user_can_send_message(me, MessageLikeEventType::CallInvite));
    let one_to_one = kind == RoomKind::Direct && room.active_members_count() == 2 && may_call;
    let mut participants: Vec<String> = room.active_room_call_participants().iter().map(ToString::to_string).collect();
    // matrix_2_0 members, from their sticky memberships.
    for member in sticky {
        participants.push(member.user_id);
    }
    crate::calls::RoomCall { via: crate::calls::via(rtc_service, may_join, one_to_one), participants }
}

pub(crate) async fn space_summary(room: &Room) -> SpaceSummary {
    let mut children = crate::admin::children(room).await;
    crate::admin::sort_children(&mut children);
    let children = children.into_iter().map(|child| child.room_id).collect();
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
        can_change_settings: power.as_ref().is_some_and(|power| crate::RoomRights::of(power, me, false, true).any()),
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

pub(crate) fn one_to_one_direct_chat(room: &Room) -> bool {
    room.direct_targets_length() == 1 && room.active_members_count() <= 2
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
        one_to_one: one_to_one_direct_chat(room),
        event_id: event.event_id.to_string(),
        sender_name: sender_name(room, &event.sender).await,
        body: content(&event.content.msgtype).plain().into_owned(),
    })
}

impl MediaRef {
    pub(crate) fn of(source: &MediaSource) -> Option<Self> {
        serde_json::to_string(source).ok().map(Self)
    }

    /// Whether the media is an encrypted file (from an encrypted room): copying it into an image
    /// pack uploads it unencrypted, which the apps ask about first.
    ///
    /// ```
    /// assert!(!matrix_core::MediaRef::from_mxc("mxc://example.org/cat").is_some_and(|media| media.is_encrypted()));
    /// ```
    pub fn is_encrypted(&self) -> bool {
        matches!(media_source(self), Ok(MediaSource::Encrypted(_)))
    }

    /// The handle for unencrypted media at an `mxc://` URI, as messages' HTML names it (a
    /// custom emoji's image); `None` for anything else.
    ///
    /// ```
    /// assert!(matrix_core::MediaRef::from_mxc("mxc://example.org/cat").is_some());
    /// assert!(matrix_core::MediaRef::from_mxc("https://example.org/cat.png").is_none());
    /// ```
    pub fn from_mxc(url: &str) -> Option<Self> {
        let url = matrix_sdk::ruma::OwnedMxcUri::from(url);
        url.is_valid().then(|| avatar(url)).flatten()
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
        sticker: true,
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
        sticker: false,
    };
    let dimensions = |w: Option<UInt>, h: Option<UInt>| Some((u32::try_from(w?).ok()?, u32::try_from(h?).ok()?));
    let thumbnail = |source: Option<&MediaSource>| source.and_then(MediaRef::of);
    let attachment = match msgtype {
        MessageType::Text(c) => return text(&c.body, &c.formatted),
        MessageType::Notice(c) => return text(&c.body, &c.formatted),
        MessageType::Emote(c) => {
            return MessageContent::Emote {
                body: c.body.clone(),
                html: c.formatted.as_ref().filter(|f| f.format == MessageFormat::Html).map(|f| f.body.clone()),
            };
        }
        MessageType::Location(c) => {
            let description = c.body.trim();
            // A body that only repeats the coordinates says nothing more.
            let description = (!description.is_empty()
                && !description.starts_with("geo:")
                && !description.contains(c.geo_uri.trim_start_matches("geo:").split(';').next().unwrap_or_default()))
            .then(|| description.to_owned());
            return MessageContent::Location(crate::model::Location {
                live_until: None,
                geo_uri: c.geo_uri.clone(),
                description,
                map: crate::geo::map(&c.geo_uri),
            });
        }
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

    /// A message as the timeline gives it: its HTML sanitised as matrix-sdk-ui does.
    fn as_the_timeline_reads(content: serde_json::Value) -> MessageContent {
        use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
        use matrix_sdk::ruma::html::{HtmlSanitizerMode, RemoveReplyFallback};
        let mut message: RoomMessageEventContent = serde_json::from_value(content).expect("a message");
        message.msgtype.sanitize(HtmlSanitizerMode::Compat, RemoveReplyFallback::Yes);
        super::content(&message.msgtype)
    }

    #[test]
    fn a_received_custom_emoji_should_reach_the_app_as_a_pill() {
        let content = as_the_timeline_reads(serde_json::json!({
            "msgtype": "m.text",
            "body": "Shipped :party:",
            "format": "org.matrix.custom.html",
            "formatted_body": "Shipped <img data-mx-emoticon src=\"mxc://x.org/party\" alt=\":party:\" height=\"32\">"
        }));
        let MessageContent::Text { html: Some(html), .. } = content else { panic!("formatted text") };
        let pills = crate::html::to_pills(&html, crate::html::Pills::Display);
        assert!(pills.contains("<span data-pill=\"emote:mxc://x.org/party\">:party:</span>"), "{pills}");
    }

    #[test]
    fn a_received_spoiler_and_formula_should_reach_the_app_as_pills() {
        let content = as_the_timeline_reads(serde_json::json!({
            "msgtype": "m.text",
            "body": "x",
            "format": "org.matrix.custom.html",
            "formatted_body": "<span data-mx-spoiler>hidden</span> <span data-mx-maths=\"x^2\"><code>x^2</code></span>"
        }));
        let MessageContent::Text { html: Some(html), .. } = content else { panic!("formatted text") };
        let pills = crate::html::to_pills(&html, crate::html::Pills::Display);
        assert!(pills.contains("data-pill=\"spoiler\"") && pills.contains("data-pill=\"math:x^2\""), "{pills}");
    }

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

    fn event(value: serde_json::Value) -> TimelineEvent {
        TimelineEvent::from_plaintext(serde_json::from_value(value).expect("timeline event"))
    }

    #[tokio::test]
    async fn a_direct_chat_named_from_its_partner_should_identify_that_name_source() {
        let (calculated, explicit) =
            (direct_room_summary(None).await, direct_room_summary(Some("Design, Support")).await);
        assert_eq!((calculated.name.as_str(), calculated.name_from_member), ("Hopper, Grace", true));
        assert_eq!((explicit.name.as_str(), explicit.name_from_member), ("Design, Support", false));
    }

    #[tokio::test]
    async fn events_with_the_same_timestamp_should_produce_distinct_room_summaries() {
        use matrix_sdk_base::{RoomInfoNotableUpdateReasons, latest_event::LatestEventValue};

        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = crate::Account::for_test(&server.url()).await;
        let client = account.client();
        client.sync_once(matrix_sdk::config::SyncSettings::default()).await.expect("initial sync");
        let room = client.get_room(matrix_sdk::ruma::room_id!("!room:example.org")).expect("room");
        let settings = client.notification_settings().await;
        let mut summaries = Vec::new();
        for event_id in ["$old", "$new"] {
            let message = event(serde_json::json!({
                "type": "m.room.message", "event_id": event_id, "sender": "@b:example.org",
                "origin_server_ts": 1, "content": {"msgtype": "m.text", "body": "Hello"}
            }));
            room.update_and_save_room_info(|mut info| {
                info.set_latest_event(LatestEventValue::Remote(message));
                (info, RoomInfoNotableUpdateReasons::LATEST_EVENT)
            })
            .await
            .expect("latest event");
            summaries.push(room_summary(&room, &settings, None, false, false, &Default::default()).await);
        }
        assert_eq!(summaries[0].activity_ms, summaries[1].activity_ms);
        assert_eq!(summaries[0].latest_event_id.as_deref(), Some("$old"));
        assert_eq!(summaries[1].latest_event_id.as_deref(), Some("$new"));
        assert_ne!(summaries[0], summaries[1]);
    }

    async fn direct_room_summary(name: Option<&'static str>) -> RoomSummary {
        use serde_json::json;

        let server = crate::test_server::Server::new(move |request| {
            let mut reply = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                reply["account_data"] = json!({"events": [
                    {"type": "m.direct", "content": {"@b:example.org": ["!room:example.org"]}}
                ]});
                let room = &mut reply["rooms"]["join"]["!room:example.org"];
                room["summary"]["m.heroes"] = json!(["@b:example.org"]);
                let state = room["state"]["events"].as_array_mut().expect("room state");
                state.push(json!({
                    "type": "m.room.member", "state_key": "@b:example.org", "sender": "@b:example.org",
                    "event_id": "$partner", "origin_server_ts": 0,
                    "content": {"membership": "join", "displayname": "Hopper, Grace"}
                }));
                if let Some(name) = name {
                    state.push(json!({
                        "type": "m.room.name", "state_key": "", "sender": "@a:example.org",
                        "event_id": "$name", "origin_server_ts": 0, "content": {"name": name}
                    }));
                }
            }
            reply
        });
        let account = crate::Account::for_test(&server.url()).await;
        let client = account.client();
        client.sync_once(matrix_sdk::config::SyncSettings::default()).await.expect("sync direct chat");
        let room = client.get_room(matrix_sdk::ruma::room_id!("!room:example.org")).expect("direct chat");
        room_summary(&room, &client.notification_settings().await, None, false, false, &Default::default()).await
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
