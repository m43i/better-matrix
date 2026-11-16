// Which timeline is on screen, so notifications for it stay quiet and it can be marked read.
// Set by the room and thread screens while focused; not part of `State`, since no view redraws
// on it.

import type { AccountId, TimelineKey } from '@better-matrix/matrix-native/types';

let focused: { account: AccountId; key: TimelineKey } | null = null;

export function setFocused(account: AccountId, key: TimelineKey): void {
  focused = { account, key };
}

/** Clears the focus, if it is still `key`'s (a newer screen may have taken it). */
export function clearFocused(account: AccountId, key: TimelineKey): void {
  if (
    focused &&
    focused.account === account &&
    focused.key.room_id === key.room_id &&
    focused.key.thread === key.thread
  ) {
    focused = null;
  }
}

/** Whether `roomId` of `account` is on screen (its room or one of its threads). */
export function roomOnScreen(account: AccountId, roomId: string): boolean {
  return focused !== null && focused.account === account && focused.key.room_id === roomId;
}
