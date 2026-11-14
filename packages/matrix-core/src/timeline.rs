//! Room timelines from matrix-sdk-ui: the ordered list of a room's messages, built from the
//! on-disk event cache and the server, with gaps from time offline filled in place as the user
//! scrolls back. This module turns its items and diffs into model values; the engine owns the
//! subscriptions.

use std::sync::Arc;

use eyeball_im::VectorDiff;
use matrix_sdk::ruma::UserId;
use matrix_sdk_ui::timeline::{
    EmbeddedEvent, EventTimelineItem, MsgLikeContent, MsgLikeKind, Profile, TimelineDetails, TimelineItem,
    TimelineItemContent, TimelineItemKind, VirtualTimelineItem,
};

use crate::convert;
use crate::model::{
    CallCard, EntryKind, Message, MessageContent, Person, Quote, Reaction, RoomChange, RoomEvent, ThreadSummary,
    TimelineDiff, TimelineEntry,
};

pub(crate) fn entry(item: &Arc<TimelineItem>, me: &UserId) -> TimelineEntry {
    let kind = match item.kind() {
        TimelineItemKind::Event(event) => match message(event, me) {
            Some(message) => EntryKind::Message(Box::new(message)),
            None => room_event(event).map_or(EntryKind::Other, |e| EntryKind::RoomEvent(Box::new(e))),
        },
        TimelineItemKind::Virtual(VirtualTimelineItem::DateDivider(at)) => EntryKind::DateDivider(u64::from(at.0)),
        TimelineItemKind::Virtual(VirtualTimelineItem::ReadMarker) => EntryKind::ReadMarker,
        TimelineItemKind::Virtual(VirtualTimelineItem::TimelineStart) => EntryKind::Start,
    };
    TimelineEntry { id: item.unique_id().0.clone(), kind }
}

fn name(sender: &UserId, profile: &TimelineDetails<Profile>) -> String {
    match profile {
        TimelineDetails::Ready(profile) => profile.display_name.clone().unwrap_or_else(|| sender.to_string()),
        _ => sender.to_string(),
    }
}

/// The plain text of an embedded (replied-to or latest-in-thread) event.
fn embedded_body(event: &EmbeddedEvent) -> Option<String> {
    match &event.content {
        TimelineItemContent::MsgLike(MsgLikeContent { kind: MsgLikeKind::Message(message), .. }) => {
            Some(convert::content(message.msgtype()).plain().to_owned())
        }
        _ => None,
    }
}

/// A poll's results as the app shows them.
fn poll_view(results: &matrix_sdk_ui::timeline::PollResult, me: &UserId) -> crate::model::Poll {
    use matrix_sdk::ruma::events::poll::start::PollKind;
    let voters: std::collections::HashSet<&String> = results.votes.values().flatten().collect();
    crate::model::Poll {
        question: results.question.clone(),
        answers: results
            .answers
            .iter()
            .map(|answer| {
                let votes = results.votes.get(&answer.id);
                crate::model::PollAnswer {
                    id: answer.id.clone(),
                    text: answer.text.clone(),
                    votes: votes.map_or(0, |v| u32::try_from(v.len()).unwrap_or(u32::MAX)),
                    mine: votes.is_some_and(|v| v.iter().any(|voter| voter == me.as_str())),
                }
            })
            .collect(),
        disclosed: results.kind == PollKind::Disclosed,
        max_selections: u32::try_from(results.max_selections).unwrap_or(1).max(1),
        voters: u32::try_from(voters.len()).unwrap_or(u32::MAX),
        ended: results.end_time.is_some(),
    }
}

/// The event's own content as JSON, for what the SDK doesn't parse.
fn raw_content(event: &EventTimelineItem) -> serde_json::Value {
    event
        .original_json()
        .and_then(|raw| raw.get_field::<serde_json::Value>("content").ok().flatten())
        .unwrap_or_default()
}

/// A call's card: the stable ring the SDK parses, the unstable one Element Call sends (which the
/// timeline's filter lets through as a custom event), or a one-to-one invite.
fn call_card(event: &EventTimelineItem) -> Option<CallCard> {
    match event.content() {
        TimelineItemContent::RtcNotification { call_intent, declined_by, .. } => Some(CallCard {
            video: call_intent.as_ref().map(|intent| intent.as_str() == "video"),
            declined_by: declined_by.iter().map(ToString::to_string).collect(),
        }),
        TimelineItemContent::MsgLike(MsgLikeContent { kind: MsgLikeKind::Other(other), .. })
            if other.event_type().to_string() == crate::calls::RING_EVENT =>
        {
            let video = raw_content(event)["m.call.intent"].as_str().map(|intent| intent == "video");
            Some(CallCard { video, ..CallCard::default() })
        }
        TimelineItemContent::CallInvite => {
            let video = raw_content(event)["offer"]["sdp"].as_str().map(crate::calls::has_video);
            Some(CallCard { video, ..CallCard::default() })
        }
        _ => None,
    }
}

fn message(event: &EventTimelineItem, me: &UserId) -> Option<Message> {
    if let Some(card) = call_card(event) {
        return Some(Message { content: MessageContent::Call(card), ..base_message(event) });
    }
    let TimelineItemContent::MsgLike(MsgLikeContent { kind, reactions, in_reply_to, thread_summary, .. }) =
        event.content()
    else {
        return None;
    };
    let (content, edited) = match kind {
        MsgLikeKind::Message(message) => (convert::content(message.msgtype()), message.is_edited()),
        MsgLikeKind::Sticker(sticker) => (convert::sticker(sticker.content()), false),
        MsgLikeKind::Redacted => (MessageContent::Redacted, false),
        MsgLikeKind::UnableToDecrypt(_) => (MessageContent::Undecryptable, false),
        MsgLikeKind::Poll(poll) => (MessageContent::Poll(poll_view(&poll.results(), me)), false),
        _ => return None,
    };
    let reactions = reactions
        .iter()
        .map(|(key, senders)| Reaction {
            key: key.clone(),
            count: u32::try_from(senders.len()).unwrap_or(u32::MAX),
            mine: senders.contains_key(me),
        })
        .collect();
    let reply_to = in_reply_to.as_ref().map(|reply| {
        let embedded = match &reply.event {
            TimelineDetails::Ready(event) => Some(event),
            _ => None,
        };
        Quote {
            event_id: reply.event_id.to_string(),
            sender_name: embedded.map(|e| name(&e.sender, &e.sender_profile)),
            body: embedded.and_then(|e| embedded_body(e)),
        }
    });
    let thread = thread_summary.as_ref().map(|summary| {
        let latest = match &summary.latest_event {
            TimelineDetails::Ready(event) => Some(event),
            _ => None,
        };
        ThreadSummary {
            replies: summary.num_replies,
            latest: latest.map(|event| Person {
                id: event.sender.to_string(),
                name: name(&event.sender, &event.sender_profile),
                avatar: match &event.sender_profile {
                    TimelineDetails::Ready(profile) => profile.avatar_url.clone().and_then(convert::avatar),
                    _ => None,
                },
            }),
            latest_at: latest.map(|event| u64::from(event.timestamp.0)),
        }
    });
    Some(Message { content, edited, reactions, reply_to, thread, ..base_message(event) })
}

/// What every message has: who sent it and when. Empty content, no reactions, no reply.
fn base_message(event: &EventTimelineItem) -> Message {
    Message {
        event_id: event.event_id().map_or_else(|| format!("local:{:?}", event.identifier()), ToString::to_string),
        sender_id: event.sender().to_string(),
        sender_name: name(event.sender(), event.sender_profile()),
        sender_avatar: match event.sender_profile() {
            TimelineDetails::Ready(profile) => profile.avatar_url.clone().and_then(convert::avatar),
            _ => None,
        },
        content: MessageContent::Redacted,
        edited: false,
        pending: event.is_local_echo(),
        timestamp_ms: u64::from(event.timestamp().0),
        reactions: Vec::new(),
        reply_to: None,
        thread: None,
    }
}

pub(crate) fn diff(diff: VectorDiff<Arc<TimelineItem>>, me: &UserId) -> TimelineDiff {
    let entry = |item: &Arc<TimelineItem>| entry(item, me);
    let all = |items: &eyeball_im::Vector<Arc<TimelineItem>>| items.iter().map(entry).collect();
    match diff {
        VectorDiff::Append { values } => TimelineDiff::Append(all(&values)),
        VectorDiff::Clear => TimelineDiff::Clear,
        VectorDiff::PushFront { value } => TimelineDiff::PushFront(entry(&value)),
        VectorDiff::PushBack { value } => TimelineDiff::PushBack(entry(&value)),
        VectorDiff::PopFront => TimelineDiff::PopFront,
        VectorDiff::PopBack => TimelineDiff::PopBack,
        VectorDiff::Insert { index, value } => TimelineDiff::Insert { index, entry: entry(&value) },
        VectorDiff::Set { index, value } => TimelineDiff::Set { index, entry: entry(&value) },
        VectorDiff::Remove { index } => TimelineDiff::Remove { index },
        VectorDiff::Truncate { length } => TimelineDiff::Truncate { length },
        VectorDiff::Reset { values } => TimelineDiff::Reset(all(&values)),
    }
}

/// How many messages are among the entries.
pub(crate) fn message_count(items: &eyeball_im::Vector<Arc<TimelineItem>>) -> usize {
    items
        .iter()
        .filter(|item| matches!(item.as_event().map(EventTimelineItem::content), Some(TimelineItemContent::MsgLike(_))))
        .count()
}

/// The event's sender as a [`Person`].
fn sender(event: &EventTimelineItem) -> Person {
    let avatar = match event.sender_profile() {
        TimelineDetails::Ready(profile) => profile.avatar_url.clone().and_then(convert::avatar),
        _ => None,
    };
    Person { id: event.sender().to_string(), name: name(event.sender(), event.sender_profile()), avatar }
}

/// A room event: a membership, profile or room-setting change. `None` for anything else, and
/// for changes not worth a line (a membership event that changed nothing).
fn room_event(event: &EventTimelineItem) -> Option<RoomEvent> {
    use matrix_sdk::ruma::events::StateEventContentChange as Change;
    use matrix_sdk_ui::timeline::{AnyOtherStateEventContentChange as Other, MembershipChange};

    let change = match event.content() {
        TimelineItemContent::MembershipChange(membership) => {
            let member = Person {
                id: membership.user_id().to_string(),
                name: membership.display_name().unwrap_or_else(|| membership.user_id().to_string()),
                avatar: membership.avatar_url().and_then(convert::avatar),
            };
            match membership.change()? {
                MembershipChange::Joined | MembershipChange::InvitationAccepted | MembershipChange::KnockAccepted => {
                    // The member joined; they are who the line is about.
                    return Some(RoomEvent { sender: member, change: RoomChange::Joined });
                }
                MembershipChange::Left => return Some(RoomEvent { sender: member, change: RoomChange::Left }),
                MembershipChange::Invited => RoomChange::Invited(member),
                MembershipChange::InvitationRevoked => RoomChange::InviteRevoked(member),
                MembershipChange::InvitationRejected => RoomChange::InviteDeclined,
                MembershipChange::Kicked => RoomChange::Removed(member),
                MembershipChange::Banned | MembershipChange::KickedAndBanned => RoomChange::Banned(member),
                MembershipChange::Unbanned => RoomChange::Unbanned(member),
                MembershipChange::Knocked => RoomChange::Knocked,
                _ => return None,
            }
        }
        TimelineItemContent::ProfileChange(profile) => {
            match (profile.displayname_change(), profile.avatar_url_change()) {
                (Some(name), _) => RoomChange::DisplayName(name.new.clone()),
                (None, Some(_)) => RoomChange::ProfilePicture,
                (None, None) => return None,
            }
        }
        TimelineItemContent::OtherState(state) => match state.content() {
            Other::RoomCreate(_) => RoomChange::Created,
            Other::RoomName(Change::Original { content, .. }) => RoomChange::RoomName(content.name.clone()),
            Other::RoomTopic(Change::Original { content, .. }) => RoomChange::RoomTopic(content.topic.clone()),
            Other::RoomAvatar(_) => RoomChange::RoomPicture,
            Other::RoomEncryption(_) => RoomChange::Encrypted,
            Other::RoomHistoryVisibility(Change::Original { content, .. }) => {
                RoomChange::HistoryVisibility(convert::history_visibility(content.history_visibility.clone()))
            }
            Other::RoomJoinRules(_) => RoomChange::JoinRules,
            Other::RoomPowerLevels(_) => RoomChange::Permissions,
            Other::RoomPinnedEvents(_) => RoomChange::PinnedMessages,
            Other::RoomTombstone(_) => RoomChange::Upgraded,
            Other::RoomCanonicalAlias(_) => RoomChange::Addresses,
            Other::RoomGuestAccess(_) => RoomChange::GuestAccess,
            Other::RoomServerAcl(_) => RoomChange::ServerAccess,
            Other::SpaceChild(_) => RoomChange::SpaceRooms,
            Other::SpaceParent(_) => RoomChange::Spaces,
            Other::RoomThirdPartyInvite(Change::Original { content, .. }) => {
                RoomChange::InvitedByContact(content.display_name.clone())
            }
            Other::PolicyRuleRoom(_) | Other::PolicyRuleServer(_) | Other::PolicyRuleUser(_) => {
                RoomChange::ModerationRules
            }
            _ => return None,
        },
        TimelineItemContent::MsgLike(MsgLikeContent { kind: MsgLikeKind::LiveLocation(_), .. }) => {
            RoomChange::LiveLocation
        }
        _ => return None,
    };
    Some(RoomEvent { sender: sender(event), change })
}
