// What every row of one timeline shares: whose timeline it is, and what rows ask of the list
// (scroll to a quoted message, open a message's actions). Stable for the life of the list, so
// rows redraw only when their own data changes. The spoilers tapped open are shared apart, so
// a tap redraws spoilers only.

import type { AccountId, Message, TimelineKey } from '@better-matrix/matrix-native/types';
import { createContext, useCallback, useContext, useMemo, useState } from 'react';

import { NONE_REVEALED, type Revealed, revealSpoiler } from '../../state/spoilers';

export interface TimelineContextValue {
  account: AccountId;
  timelineKey: TimelineKey;
  /** The thread's root, in a thread's timeline. */
  thread: string | null;
  /** Scrolls to a message, if it is loaded, and highlights it. */
  scrollTo: (eventId: string) => void;
  /** Opens a message's actions (a long press). */
  openActions: (message: Message) => void;
  /** Whether the timeline's screen is still on show: a link's late answer opens nothing then. */
  shown: () => boolean;
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

/** The spoilers tapped open on a screen (`state/spoilers.ts`), and how a tap shows one. */
export interface SpoilersValue {
  revealed: Revealed;
  reveal: (id: string) => void;
}

/** Outside a screen that keeps them, spoilers stay hidden. */
export const SpoilerContext = createContext<SpoilersValue>({
  revealed: NONE_REVEALED,
  reveal: () => undefined,
});

/** The spoilers a screen keeps shown while it is mounted, for its `SpoilerContext`. */
export function useSpoilers(): SpoilersValue {
  const [revealed, setRevealed] = useState(NONE_REVEALED);
  const reveal = useCallback((id: string) => setRevealed((now) => revealSpoiler(now, id)), []);

  return useMemo(() => ({ revealed, reveal }), [revealed, reveal]);
}

/** A spoiler on screen: whether it was tapped open, and how a tap opens it. */
export interface SpoilerView {
  shown: boolean;
  reveal: () => void;
}

/** The spoiler `id` as its screen keeps it. */
export function useSpoiler(id: string): SpoilerView {
  const { revealed, reveal } = useContext(SpoilerContext);

  return { shown: revealed.has(id), reveal: () => reveal(id) };
}
