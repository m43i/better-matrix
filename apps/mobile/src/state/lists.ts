// What the shell's lists show besides sections: a conversation's preview line and time, the
// rooms "Jump to…" finds, and the Activity tab's rows. Pure, so every list derives them alike.

import type { AccountId, Message, Notification, RoomSummary, SpaceSummary } from '@better-matrix/matrix-native/types';

import { locale, t } from '../i18n';
import { byRecency } from './sections';
import type { AccountData, State } from './types';

/** A message as one line of plain text, as the desktop's overview shows it. */
export function messageText(message: Message): string {
  const content = message.content;
  switch (content.type) {
    case 'Text':
      return content.data.body.replace(/\s+/g, ' ').trim();
    case 'Attachment':
      return content.data.name;
    case 'Poll':
      return content.data.question;
    case 'Redacted':
      return t('message.deleted');
    case 'Undecryptable':
      return t('message.undecryptable');
    case 'Call':
      return content.data.video === true
        ? t('call.card.video')
        : content.data.video === false
          ? t('call.card.voice')
          : t('call.card.call');
  }
}

/** "Sender: text", the preview under a conversation's name. */
export function previewLine(message: Message): string {
  return t('overview.preview', { sender: message.sender_name, body: messageText(message) });
}

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/**
 * When a conversation's latest message came, as a list shows it: the clock time today,
 * "Yesterday", the weekday within a week, else the date.
 */
export function listTime(ms: number, now: number): string {
  const days = Math.round((startOfDay(now) - startOfDay(ms)) / DAY_MS);
  const lang = locale();
  if (days <= 0) return new Intl.DateTimeFormat(lang, { hour: 'numeric', minute: '2-digit' }).format(ms);
  if (days === 1) return t('date.yesterday');
  if (days < 7) return new Intl.DateTimeFormat(lang, { weekday: 'short' }).format(ms);
  const sameYear = new Date(ms).getFullYear() === new Date(now).getFullYear();
  return new Intl.DateTimeFormat(lang, sameYear ? { day: 'numeric', month: 'short' } : { dateStyle: 'short' }).format(ms);
}

/** A name folded for matching: lower case, without accents. */
function fold(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
}

/** How well `name` matches `term`: 0 at its start, 1 at a word's start, 2 inside; null if not. */
function matchRank(name: string, term: string): number | null {
  const n = fold(name);
  const at = n.indexOf(term);
  if (at < 0) return null;
  if (at === 0) return 0;
  return /[\s\-_#@.:]/.test(n[at - 1] ?? '') ? 1 : 2;
}

export interface JumpResults {
  rooms: RoomSummary[];
  direct: RoomSummary[];
  spaces: SpaceSummary[];
}

/**
 * What "Jump to…" lists for `term`: rooms, direct chats and spaces whose name holds it, best
 * matches first, then the latest conversation. Without a term, the latest conversations.
 */
export function jumpTo(account: AccountData, term: string, now: number, recentLimit = 8): JumpResults {
  const query = fold(term.trim());
  if (!query) {
    const recent = byRecency(account, account.rooms, now).slice(0, recentLimit);
    return { rooms: recent.filter((r) => r.kind === 'Group'), direct: recent.filter((r) => r.kind === 'Direct'), spaces: [] };
  }
  const ranked = (rooms: readonly RoomSummary[]) => {
    const recency = new Map(byRecency(account, rooms, now).map((r, i) => [r.room_id, i]));
    return rooms
      .flatMap((room) => {
        const ids = room.kind === 'Direct' ? room.dm_users : [];
        const ranks = [matchRank(room.name, query), ...ids.map((id) => matchRank(id, query))].filter((r) => r !== null);
        return ranks.length > 0 ? [{ room, rank: Math.min(...ranks) }] : [];
      })
      .sort((a, b) => a.rank - b.rank || (recency.get(a.room.room_id) ?? 0) - (recency.get(b.room.room_id) ?? 0))
      .map((x) => x.room);
  };
  const spaces = account.spaces
    .flatMap((space) => {
      const rank = matchRank(space.name, query);
      return rank === null ? [] : [{ space, rank }];
    })
    .sort((a, b) => a.rank - b.rank)
    .map((x) => x.space);
  return {
    rooms: ranked(account.rooms.filter((r) => r.kind === 'Group')),
    direct: ranked(account.rooms.filter((r) => r.kind === 'Direct')),
    spaces,
  };
}

/** One row of the Activity tab. */
export interface ActivityRow {
  notification: Notification;
  /** The room as the list knows it, for its picture and kind. */
  room: RoomSummary | undefined;
  /** The account's name, when several are signed in. */
  accountLabel: string | null;
}

/** The Activity tab: every account's notifications, newest first, labelled when there are several. */
export function activityRows(state: Pick<State, 'activity' | 'accounts'>): ActivityRow[] {
  const several = state.accounts.length > 1;
  const byAccount = new Map<AccountId, AccountData>(state.accounts.map((a) => [a.summary.id, a]));
  return state.activity.flatMap((notification) => {
    const account = byAccount.get(notification.account);
    // Notifications of an account signed out since are gone with it.
    if (!account) return [];
    return [
      {
        notification,
        room: account.rooms.find((r) => r.room_id === notification.room_id),
        accountLabel: several ? account.summary.display_name || account.summary.id : null,
      },
    ];
  });
}
