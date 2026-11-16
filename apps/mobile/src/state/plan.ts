// The timeline plan: a timeline's rows, worked out in one pass over its entries before any row
// is drawn (the desktop's `plan` in ui/timeline.rs). Messages from one sender close together
// share a header, joins and leaves in a row merge into one line, and the list shows where
// visible history starts, a failed or loading page, date dividers and the "New" marker. The
// list draws only the rows near the screen, each by its plan entry.

import type { Message, RoomEvent, TimelineEntry } from '@better-matrix/matrix-native/types';

import type { ShownMarker } from './newMarker';
import type { Timeline } from './types';

/** Messages from one sender this close together share one header. */
export const GROUP_WINDOW_MS = 5 * 60_000;

export type Row =
  /** Where the room's visible history begins. */
  | { kind: 'start'; key: string }
  | { kind: 'historyFailed'; key: string; error: string }
  | { kind: 'loading'; key: string }
  /**
   * A message; `continues` the one before it (same sender, close in time) or not. `latestCall`:
   * the newest call card, the one that shows whether the room's call is under way.
   */
  | { kind: 'message'; key: string; message: Message; continues: boolean; latestCall: boolean }
  /** Room events shown as one line: one, or a run of joins (or leaves). */
  | { kind: 'events'; key: string; events: RoomEvent[] }
  | { kind: 'date'; key: string; at: number }
  /** The "New" marker: where the unread messages begin; `fading` once read past. */
  | { kind: 'new'; key: string; fading: boolean }
  /** Under a thread's root: how many replies follow. */
  | { kind: 'replies'; key: string; count: number };

export type RowKind = Row['kind'];

export interface PlanOptions {
  /** The thread's root event when the timeline is a thread's. */
  thread: string | null;
  /** The thread's root as the room's timeline has it, shown until the thread's own arrives. */
  root?: Message | null;
  /**
   * The "New" divider to draw (`state/newMarker.ts`), in place of the SDK's read marker, which
   * moves as receipts advance. Left out, the SDK's marker is drawn where it is.
   */
  marker?: ShownMarker | null;
}

function isStart(entry: TimelineEntry): boolean {
  return entry.kind.type === 'Start';
}

/** Whether there is nothing older to load: the room's start, or a thread's root, is in. */
export function atStart(timeline: Timeline, thread: string | null): boolean {
  if (timeline.reachedStart) return true;
  return timeline.entries.some((entry) =>
    thread === null ? isStart(entry) : entry.kind.type === 'Message' && entry.kind.data.event_id === thread,
  );
}

/** Joins (or leaves) in a row read as one line. */
function groups(event: RoomEvent): boolean {
  return event.change.type === 'Joined' || event.change.type === 'Left';
}

/** The rows of a timeline, oldest first. */
export function planTimeline(timeline: Timeline, options: PlanOptions): Row[] {
  const { thread } = options;
  const inThread = thread !== null;
  const entries = timeline.entries;
  const rows: Row[] = [];
  const prefix = inThread ? 'thread-' : '';
  const hasStart = entries.some(isStart);
  if (!inThread && timeline.reachedStart && !hasStart) rows.push({ kind: 'start', key: 'timeline-start' });
  if (timeline.historyError !== null) {
    rows.push({ kind: 'historyFailed', key: `${prefix}timeline-history-failed`, error: timeline.historyError });
  } else if (timeline.loadingOlder && !atStart(timeline, thread)) {
    rows.push({ kind: 'loading', key: `${prefix}timeline-loading` });
  }
  const rootLoaded = inThread && entries.some((e) => e.kind.type === 'Message' && e.kind.data.event_id === thread);
  // The thread's own copy of its root may lack the summary: the replies loaded count too.
  const loadedReplies = inThread
    ? entries.filter((e) => e.kind.type === 'Message' && e.kind.data.event_id !== thread).length
    : 0;
  let previous: Message | null = null;
  const pushMessage = (key: string, message: Message) => {
    const prev: Message | null = previous;
    const continues =
      prev !== null &&
      prev.sender_id === message.sender_id &&
      message.timestamp_ms - prev.timestamp_ms < GROUP_WINDOW_MS &&
      message.reply_to === null;
    rows.push({ kind: 'message', key, message, continues, latestCall: false });
    previous = message;
    if (inThread && message.event_id === thread) {
      rows.push({ kind: 'replies', key: 'thread-replies', count: Math.max(message.thread?.replies ?? 0, loadedReplies) });
      // Replies start their own run under the root.
      previous = null;
    }
  };
  if (inThread && !rootLoaded && options.root) pushMessage(`${prefix}msg-root`, options.root);
  const marker = options.marker;
  let index = 0;
  for (let entry = entries[0]; entry !== undefined; entry = entries[index]) {
    index += 1;
    const kind = entry.kind;
    if (marker && entry.id === marker.boundary) {
      rows.push({ kind: 'new', key: `${prefix}new-marker`, fading: marker.fading });
    }
    switch (kind.type) {
      case 'RoomEvent': {
        const events = [kind.data];
        // Hidden entries between joins don't break the run.
        // A run stops at the "New" divider, which stands between two of its events.
        const atMarker = () => marker != null && entries[index]?.id === marker.boundary;
        for (let next = entries[index]?.kind; groups(kind.data) && next !== undefined && !atMarker(); next = entries[index]?.kind) {
          if (next.type === 'RoomEvent' && next.data.change.type === kind.data.change.type) events.push(next.data);
          else if (next.type !== 'Other') break;
          index += 1;
        }
        rows.push({ kind: 'events', key: `${prefix}${entry.id}`, events });
        previous = null;
        break;
      }
      case 'Message':
        pushMessage(`${prefix}msg-${entry.id}`, kind.data);
        break;
      case 'DateDivider':
        rows.push({ kind: 'date', key: `${prefix}${entry.id}`, at: kind.data });
        previous = null;
        break;
      case 'ReadMarker':
        if (marker === undefined) rows.push({ kind: 'new', key: `${prefix}${entry.id}`, fading: false });
        break;
      case 'Start':
        if (!inThread) {
          rows.push({ kind: 'start', key: entry.id });
          previous = null;
        }
        break;
      case 'Other':
        break;
    }
  }
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i];
    if (row?.kind === 'message' && row.message.content.type === 'Call') {
      rows[i] = { ...row, latestCall: true };
      break;
    }
  }
  return rows;
}

/** The row showing the message with this event ID. */
export function rowOf(rows: readonly Row[], eventId: string): number {
  return rows.findIndex((row) => row.kind === 'message' && row.message.event_id === eventId);
}

/**
 * The newest message the server has, which marking the timeline read reaches: once it is on
 * screen and differs from the one last marked, the timeline is marked read again.
 */
export function newestSent(entries: readonly TimelineEntry[]): string | null {
  for (let i = entries.length - 1; i >= 0; i--) {
    const kind = entries[i]?.kind;
    if (kind?.type === 'Message' && !kind.data.pending && !kind.data.event_id.startsWith('local:')) return kind.data.event_id;
  }
  return null;
}

/** What marking read last sent: up to which message, and whether the fully-read marker moved. */
export interface Marked {
  eventId: string;
  fullyRead: boolean;
}

/**
 * Whether to mark read now: on screen at the newest, with a newer message than last marked, or
 * with the fully-read marker now allowed to follow (`fullyRead`) where it wasn't last time.
 * `positioned`: the list stands where it opens, at a linked message when it opened at one; until
 * then, being "at the newest" is only where the list starts drawing, not what the reader sees.
 */
export function shouldMarkRead(options: {
  focused: boolean;
  positioned: boolean;
  atLatest: boolean;
  newest: string | null;
  fullyRead: boolean;
  lastMarked: Marked | null;
}): boolean {
  const { focused, positioned, atLatest, newest, fullyRead, lastMarked } = options;
  if (!focused || !positioned || !atLatest || newest === null) return false;
  return newest !== lastMarked?.eventId || (fullyRead && !lastMarked.fullyRead);
}
