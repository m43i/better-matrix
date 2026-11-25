// Plain values for tests, as the engine would report them.

import type {
  AccountSummary,
  Message,
  MessageContent,
  Person,
  RoomChange,
  RoomRights,
  RoomSettings,
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
    note_to_self: false,
    avatar: null,
    member_count: 2,
    active_member_count: 2,
    history: 'Shared',
    notifications: 'AllMessages',
    custom_notifications: false,
    call: { via: null, participants: [] },
    activity_ms: null,
    encrypted: true,
    public: false,
    pinned: [],
    can_pin: true,
    saved: [],
    can_start_meeting: false,
    can_rename: true,
    priority: 'Normal',
    marked_unread: false,
    can_change_settings: true,
    knocks: 0,
    successor: null,
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
    highlighted: false,
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

export function roomEventEntry(
  id: string,
  sender: Person,
  change: RoomChange,
  timestamp_ms = 1,
): TimelineEntry {
  return { id, kind: { type: 'RoomEvent', data: { sender, change, timestamp_ms } } };
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

/** Every right to a room's settings, or none with `false`. */
export function rights(all = true): RoomRights {
  return {
    name: all,
    topic: all,
    avatar: all,
    join_rule: all,
    history: all,
    guest_access: all,
    addresses: all,
    encryption: all,
    permissions: all,
    upgrade: all,
    space_rooms: all,
  };
}

/** A private, unencrypted room's settings as an admin (level 100) sees them. */
export function roomSettings(room_id: string, change: Partial<RoomSettings> = {}): RoomSettings {
  return {
    room_id,
    space: false,
    name: 'Lab',
    topic: null,
    avatar: null,
    join_rule: 'Invite',
    can_restrict: false,
    can_knock: true,
    history: 'Shared',
    guest_access: false,
    encrypted: false,
    main_address: null,
    addresses: [],
    published: false,
    server: 'example.org',
    version: '11',
    upgrade_to: null,
    permissions: [
      { permission: 'Invite', level: 0, editable: true },
      { permission: 'Ban', level: 50, editable: true },
      { permission: 'Upgrade', level: 150, editable: false },
    ],
    own_level: 100,
    rights: rights(),
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
  return {
    room_id,
    name,
    avatar: null,
    can_add_rooms: true,
    can_rename: true,
    can_change_settings: true,
    children,
    members,
  };
}

/** Every character from `first` to `last`, in order. */
function codePoints(first: number, last: number): string {
  let text = '';

  for (let c = first; c <= last; c++) text += String.fromCodePoint(c);

  return text;
}

/** The BMP's whole private use area, as a pasted run of icons might hold it. */
export const BMP_PRIVATE_USE = codePoints(0xe000, 0xf8ff);

/** Every private-use character Unicode has: no placeholder is left for a pill. */
export const EVERY_PRIVATE_USE =
  BMP_PRIVATE_USE + codePoints(0xf0000, 0xffffd) + codePoints(0x100000, 0x10fffd);
