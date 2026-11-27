// What a timeline says in words, as the desktop says it (apps/desktop/src/ui/timeline.rs): dates
// and times, room events, file sizes, who is typing, quotes and call cards. Pure: the language
// is the catalog's, times are this device's local time.

import type {
  CallCard,
  Person,
  Quote,
  RoomEvent,
  RoomKind,
} from '@better-matrix/matrix-native/types';

import { personName } from './names';
import { type Key, locale, t, tp } from '../i18n';

/** `HH:MM` in local time. */
export function clockTime(ms: number): string {
  const at = new Date(ms);

  return `${String(at.getHours()).padStart(2, '0')}:${String(at.getMinutes()).padStart(2, '0')}`;
}

/** Days between two local dates (0 for the same day, 1 when `at` is yesterday). */
function daysBefore(at: number, now: number): number {
  const day = (ms: number) => {
    const d = new Date(ms);

    return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 86_400_000;
  };

  return day(now) - day(at);
}

const MONTHS = [
  'date.month_01',
  'date.month_02',
  'date.month_03',
  'date.month_04',
  'date.month_05',
  'date.month_06',
  'date.month_07',
  'date.month_08',
  'date.month_09',
  'date.month_10',
  'date.month_11',
  'date.month_12',
] as const satisfies readonly Key[];

/** "Today", "Yesterday", or the date spelled out. */
export function dateLabel(at: number, now: number): string {
  const days = daysBefore(at, now);

  if (days === 0) return t('date.today');

  if (days === 1) return t('date.yesterday');
  const date = new Date(at);

  return t('date.format', {
    day: String(date.getDate()),
    month: t(MONTHS[date.getMonth()] ?? 'date.month_01'),
    year: String(date.getFullYear()),
  });
}

/** "Last reply today at 09:51", or yesterday, or on a date. */
export function lastReplyText(at: number, now: number): string {
  const time = clockTime(at);
  const days = daysBefore(at, now);

  if (days === 0) return t('thread.last_reply_today', { time });

  if (days === 1) return t('thread.last_reply_yesterday', { time });

  return t('thread.last_reply', { date: dateLabel(at, now), time });
}

/** A date divider's and the preview's "Today at 09:51"-style line. */
export function sentAt(at: number, now: number): string {
  return `${dateLabel(at, now)} · ${clockTime(at)}`;
}

/** What one room event says. */
export function roomEventText(event: RoomEvent, reverse = false): string {
  const name = personName(event.sender.name, reverse);
  const change = event.change;

  switch (change.type) {
    case 'Created':
      return t('event.created', { name });
    case 'Joined':
      return t('event.joined', { name });
    case 'Left':
      return t('event.left', { name });
    case 'Invited':
      return t('event.invited', { name, target: personName(change.data.name, reverse) });
    case 'InviteRevoked':
      return t('event.invite_revoked', { name, target: personName(change.data.name, reverse) });
    case 'InviteDeclined':
      return t('event.invite_declined', { name });
    case 'Removed':
      return t('event.removed', { name, target: personName(change.data.name, reverse) });
    case 'Banned':
      return t('event.banned', { name, target: personName(change.data.name, reverse) });
    case 'Unbanned':
      return t('event.unbanned', { name, target: personName(change.data.name, reverse) });
    case 'Knocked':
      return t('event.knocked', { name });
    case 'DisplayName':
      return change.data === null
        ? t('event.display_name_removed', { name })
        : t('event.display_name', { name, new_name: personName(change.data, reverse) });
    case 'ProfilePicture':
      return t('event.profile_picture', { name });
    case 'RoomName':
      return t('event.room_name', { name, room: change.data });
    case 'RoomTopic':
      return t('event.room_topic', { name, topic: change.data });
    case 'RoomPicture':
      return t('event.room_picture', { name });
    case 'Encrypted':
      return t('event.encrypted', { name });
    case 'HistoryVisibility':
      return t('event.history', { name });
    case 'JoinRules':
      return t('event.join_rules', { name });
    case 'Permissions':
      return t('event.permissions', { name });
    case 'PinnedMessages':
      return t('event.pinned', { name });
    case 'Upgraded':
      return t('event.upgraded', { name });
    case 'Addresses':
      return t('event.addresses', { name });
    case 'GuestAccess':
      return t('event.guest_access', { name });
    case 'ServerAccess':
      return t('event.server_access', { name });
    case 'SpaceRooms':
      return t('event.space_rooms', { name });
    case 'Spaces':
      return t('event.spaces', { name });
    case 'InvitedByContact':
      return t('event.invited', { name, target: change.data });
    case 'ModerationRules':
      return t('event.moderation_rules', { name });
    case 'LiveLocation':
      return t('event.live_location', { name });
    case 'WidgetAdded':
      return change.data.meeting === null
        ? t('mobile.widgets.event_added', { name, widget: change.data.name })
        : t('mobile.widgets.event_meeting', { name });
    case 'WidgetRemoved':
      return t('mobile.widgets.event_removed', { name });
  }
}

/** The most names a merged line lists before "and N more". */
export const EVENT_PEOPLE = 4;

/** The people a run of room events is about, each once, in order. */
export function eventPeople(run: readonly RoomEvent[]): Person[] {
  const people: Person[] = [];

  for (const event of run) {
    if (!people.some((p) => p.id === event.sender.id)) people.push(event.sender);
  }

  return people;
}

/** What a run of joins or leaves says: "Ada, Grace and Linus joined". */
export function eventRunText(run: readonly RoomEvent[], reverse = false): string {
  const first = run[0];

  if (!first) return '';
  const people = eventPeople(run);

  if (people.length <= 1) return roomEventText(first, reverse);
  const names = people.map((p) => personName(p.name, reverse));
  let listed: string;

  if (names.length <= EVENT_PEOPLE) {
    listed = `${names.slice(0, -1).join(', ')}${t('list.and')}${names[names.length - 1] ?? ''}`;
  } else {
    listed = `${names.slice(0, EVENT_PEOPLE).join(', ')}${t('list.more', { count: names.length - EVENT_PEOPLE })}`;
  }

  return first.change.type === 'Left'
    ? t('event.left_many', { names: listed })
    : t('event.joined_many', { names: listed });
}

/** `1.4 MB`-style sizes in the locale's numbers. */
export function humanSize(bytes: number): string {
  let value = bytes;
  let unit = 0;

  while (value >= 1000 && unit < 3) {
    value /= 1000;
    unit += 1;
  }

  const digits = unit === 0 ? 0 : 1;

  const n = value.toLocaleString(locale(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });

  const key = (['size.b', 'size.kb', 'size.mb', 'size.gb'] as const)[unit] ?? 'size.gb';

  return t(key, { n });
}

/** "Ada is typing…", "Ada and Grace are typing…", "Ada, Grace and 3 others are typing…". */
export function typingText(people: readonly Person[]): string {
  const [first, second] = people;

  if (!first) return '';

  if (!second) return t('typing.one', { name: first.name });

  if (people.length === 2) return t('typing.two', { first: first.name, second: second.name });

  return t('typing.many', { first: first.name, second: second.name, count: people.length - 2 });
}

/** A call's kind: "Video call", "Voice call" or "Call". */
export function callTitle(card: CallCard): string {
  return card.video === true
    ? t('call.card.video')
    : card.video === false
      ? t('call.card.voice')
      : t('call.card.call');
}

/** What a quote says of the answered message: its text, its call's kind, or that it isn't loaded. */
export function quoteText(quote: Quote): string {
  if (quote.body !== null) return quote.body;

  if (quote.call) return callTitle(quote.call);

  return t('quote.unavailable');
}

/** "3 replies". */
export function repliesText(count: number): string {
  return tp('thread.replies', count);
}

/** A room's name inside a sentence: `#general`, or a person's name. */
export function inlineName(room: { name: string; kind: RoomKind }, name = room.name): string {
  return room.kind === 'Group' ? `#${name}` : name;
}
