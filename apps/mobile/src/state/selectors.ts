// Derived data views read: kept here so every screen derives it the same way.

import type {
  AccountId,
  MediaRef,
  MediaSize,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { EMPTY_TIMELINE, mediaKey } from './reducer';
import { timelineId } from './timeline';
import type { AccountData, State, Timeline } from './types';

export function account(state: State, id: AccountId | null): AccountData | undefined {
  return id === null ? undefined : state.accounts.find((a) => a.summary.id === id);
}

/** The account on screen. */
export function activeAccount(state: State): AccountData | undefined {
  return account(state, state.active);
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
