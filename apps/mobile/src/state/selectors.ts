// Derived data views read: kept here so every screen derives it the same way.

import type {
  AccountId,
  MediaRef,
  MediaSize,
  SavedMessage,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { EMPTY_TIMELINE, mediaKey } from './reducer';
import { newestMessages, type NewestMessages } from './sections';
import { timelineId } from './timeline';
import type { AccountData, State, Timeline } from './types';

export function account(state: State, id: AccountId | null): AccountData | undefined {
  return id === null ? undefined : state.accounts.find((a) => a.summary.id === id);
}

/** The account on screen. */
export function activeAccount(state: State): AccountData | undefined {
  return account(state, state.active);
}

/**
 * What the room lists (the Home and DMs tabs) draw from an account: of its timelines, only each
 * room's newest message (`newestMessages`).
 */
export type RoomListData = Pick<
  AccountData,
  | 'summary'
  | 'rooms'
  | 'spaces'
  | 'invitations'
  | 'roomsLoaded'
  | 'scope'
  | 'sidebar'
  | 'latest'
  | 'heardAt'
> & { newest: NewestMessages };

/**
 * The account on screen as the room lists draw it. Compared with `shallow`, it changes only when
 * one of these fields does: typing, members, threads and the like leave the lists alone, and so
 * does a timeline changing anything but a room's newest message as the list shows it.
 */
export function activeRoomList(state: State): RoomListData | undefined {
  const data = activeAccount(state);

  if (!data) return undefined;

  return {
    summary: data.summary,
    rooms: data.rooms,
    spaces: data.spaces,
    invitations: data.invitations,
    roomsLoaded: data.roomsLoaded,
    scope: data.scope,
    sidebar: data.sidebar,
    newest: newestMessages(data),
    latest: data.latest,
    heardAt: data.heardAt,
  };
}

export function timeline(state: State, id: AccountId, key: TimelineKey): Timeline {
  return account(state, id)?.timelines[timelineId(key)] ?? EMPTY_TIMELINE;
}

export function room(state: State, id: AccountId, roomId: string) {
  return account(state, id)?.rooms.find((r) => r.room_id === roomId);
}

/** A fetched media file's `file://` URI, once it's there. */
export function mediaUri(state: State, media: MediaRef, size: MediaSize): string | undefined {
  const path = state.media[mediaKey(media, size)];

  return path === undefined ? undefined : `file://${path}`;
}

/**
 * The saved messages listed, as the rooms' summaries know them now: one no longer saved leaves
 * at once. `undefined` until listed.
 */
export function savedMessages(state: State, id: AccountId): SavedMessage[] | undefined {
  const data = account(state, id);

  if (!data?.saved) return undefined;

  const saved = new Set(
    data.rooms.flatMap((r) => r.saved.map((eventId) => `${r.room_id}|${eventId}`)),
  );

  return data.saved.filter((m) => saved.has(`${m.room_id}|${m.preview.event_id}`));
}

/** Every saved message's room and event, as one string: it changes when anything is saved. */
export function savedSignature(state: State, id: AccountId): string {
  return (account(state, id)?.rooms ?? [])
    .flatMap((r) => r.saved.map((eventId) => `${r.room_id}|${eventId}`))
    .sort()
    .join(',');
}
