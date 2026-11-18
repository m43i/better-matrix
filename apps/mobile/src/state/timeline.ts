// Timeline diffs, applied as the engine's vector diffs apply: by index, in order.

import type { TimelineDiff, TimelineEntry, TimelineKey } from '@better-matrix/matrix-native/types';

/** A timeline's key as one string, for maps: the room ID, plus `|` and the thread root. */
export function timelineId(key: TimelineKey): string {
  return key.thread ? `${key.room_id}|${key.thread}` : key.room_id;
}

/** Whether a `timelineId` is a room's main timeline rather than a thread. */
export function isRoomTimeline(id: string): boolean {
  return !id.includes('|');
}

/** A room's main timeline. */
export function roomKey(roomId: string): TimelineKey {
  return { room_id: roomId, thread: null };
}

/** One thread of a room. */
export function threadKey(roomId: string, root: string): TimelineKey {
  return { room_id: roomId, thread: root };
}

/** `entries` after `diffs`, as a new array; `entries` is left as it was. */
export function applyDiffs(
  entries: readonly TimelineEntry[],
  diffs: readonly TimelineDiff[],
): TimelineEntry[] {
  let next = entries.slice();

  for (const diff of diffs) {
    switch (diff.type) {
      case 'Reset':
        next = diff.data.slice();
        break;
      case 'Append':
        next.push(...diff.data);
        break;
      case 'Clear':
        next = [];
        break;
      case 'PushFront':
        next.unshift(diff.data);
        break;
      case 'PushBack':
        next.push(diff.data);
        break;
      case 'PopFront':
        next.shift();
        break;
      case 'PopBack':
        next.pop();
        break;
      case 'Insert':
        next.splice(diff.data.index, 0, diff.data.entry);
        break;
      case 'Set':
        next[diff.data.index] = diff.data.entry;
        break;
      case 'Remove':
        next.splice(diff.data.index, 1);
        break;
      case 'Truncate':
        next.length = Math.min(next.length, diff.data.length);
        break;
    }
  }

  return next;
}
