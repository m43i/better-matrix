// What every row of one timeline shares: whose timeline it is, and what rows ask of the list
// (scroll to a quoted message, open a message's actions). Stable for the life of the list, so
// rows redraw only when their own data changes.

import type { AccountId, Message, TimelineKey } from '@better-matrix/matrix-native/types';
import { createContext, useContext } from 'react';

export interface TimelineContextValue {
  account: AccountId;
  timelineKey: TimelineKey;
  /** The thread's root, in a thread's timeline. */
  thread: string | null;
  /** Scrolls to a message, if it is loaded, and highlights it. */
  scrollTo: (eventId: string) => void;
  /** Opens a message's actions (a long press). */
  openActions: (message: Message) => void;
}

export const TimelineContext = createContext<TimelineContextValue | null>(null);

export function useTimelineContext(): TimelineContextValue {
  const value = useContext(TimelineContext);

  if (!value) throw new Error('timeline rows need a TimelineContext');

  return value;
}

/**
 * The event IDs of the room's pinned messages, read once for the list rather than once per row;
 * a new set only when the pins change.
 */
export const PinnedContext = createContext<ReadonlySet<string>>(new Set());

/** Whether the message with `eventId` is pinned in the timeline's room. */
export function usePinned(eventId: string): boolean {
  return useContext(PinnedContext).has(eventId);
}
