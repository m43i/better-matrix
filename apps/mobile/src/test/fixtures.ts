// Plain values for tests, as the engine would report them.

import type {
  AccountSummary,
  Message,
  MessageContent,
  Person,
  RoomChange,
  RoomSummary,
  SpaceSummary,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';

import type { Timeline } from '../state/types';

export const ADA = '@ada:example.org';

export const GRACE = '@grace:example.org';

export function summary(id: string = ADA, display_name = 'Ada'): AccountSummary {
  return {
    id,
    display_name,
    homeserver: 'example.org',
    default_notifications: 'AllMessages',
    avatar: null,
    sign_in: 'Password',
  };
}

export function roomSummary(
  room_id: string,
  name = room_id,
  kind: RoomSummary['kind'] = 'Group',
): RoomSummary {
  return {
    room_id,
    name,
    topic: null,
    kind,
    unread: 0,
    mentions: 0,
    dm_users: [],
    avatar: null,
    member_count: 2,
    active_member_count: 2,
    history: 'Shared',
    notifications: 'AllMessages',
    custom_notifications: false,
    call: { via: null, participants: [] },
    activity_ms: null,
    encrypted: true,
    pinned: [],
    can_pin: true,
    saved: [],
    can_rename: true,
  };
}

export function message(event_id: string, body = event_id): Message {
  return {
    event_id,
    sender_id: GRACE,
    sender_name: 'Grace',
    sender_avatar: null,
    content: { type: 'Text', data: { body, html: null } },
    edited: false,
    editable: false,
    pending: false,
    timestamp_ms: 1_700_000_000_000,
    reactions: [],
    reply_to: null,
    thread: null,
  };
}

export function entry(id: string): TimelineEntry {
  return { id, kind: { type: 'Message', data: message(id) } };
}

/** A message entry with its message changed by `change`. */
export function messageEntry(id: string, change: Partial<Message> = {}): TimelineEntry {
  return { id, kind: { type: 'Message', data: { ...message(id), ...change } } };
}

/** A message with other content. */
export function withContent(
  id: string,
  content: MessageContent,
  change: Partial<Message> = {},
): Message {
  return { ...message(id), content, ...change };
}

export function person(id: string, name = id): Person {
  return { id, name, avatar: null };
}

/** A room event entry: `sender` did `change`. */
/** An event timelines leave out unless hidden events show. */
export function hiddenEntry(id: string, eventType = 'm.room.server_acl'): TimelineEntry {
  return {
    id,
    kind: {
      type: 'Hidden',
      data: {
        event_id: `$${id}`,
        sender: person('@grace:example.org'),
        event_type: eventType,
        timestamp_ms: 1,
      },
    },
  };
}

export function roomEventEntry(id: string, sender: Person, change: RoomChange): TimelineEntry {
  return { id, kind: { type: 'RoomEvent', data: { sender, change } } };
}

/** A timeline holding `entries`, with its paging as given. */
export function timelineOf(entries: TimelineEntry[], change: Partial<Timeline> = {}): Timeline {
  return {
    entries,
    readBoundary: { kind: 'unknown' },
    reachedStart: false,
    loadingOlder: false,
    historyError: null,
    opening: false,
    openFailed: false,
    ...change,
  };
}

/** A space holding `children` (rooms or subspaces), with `members` joined. */
export function spaceSummary(
  room_id: string,
  children: string[] = [],
  members: string[] = [],
  name = room_id,
): SpaceSummary {
  return { room_id, name, avatar: null, can_add_rooms: true, can_rename: true, children, members };
}
