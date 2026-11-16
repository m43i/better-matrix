// The "New" divider, kept where it first showed (ADR 0006, as the desktop keeps it): the SDK
// moves or drops its read marker as receipts advance, and drawing the divider straight from it
// made it jump under the reader. Once shown, the divider stays between the same two entries; when
// the SDK's marker moves on, it fades out in place, keeping its height until the fade ends; after
// that no divider shows again while the timeline stays on screen.

import type { TimelineEntry } from '@better-matrix/matrix-native/types';

/** A divider on screen: before the entry `boundary`, fading out or not. */
export interface ShownMarker {
  boundary: string;
  fading: boolean;
}

export interface MarkerState {
  shown: ShownMarker | null;
  /** A divider has faded: later markers stay hidden until the timeline leaves the screen. */
  hidden: boolean;
}

export const NO_MARKER: MarkerState = { shown: null, hidden: false };

/** How long the divider fades, in milliseconds (the desktop's), and under reduced motion. */
export const MARKER_FADE_MS = 2500;
export const MARKER_FADE_REDUCED_MS = 150;

/** The entry the SDK's read marker stands before: the first message or room event after it. */
export function sdkBoundary(entries: readonly TimelineEntry[]): string | null {
  const at = entries.findIndex((entry) => entry.kind.type === 'ReadMarker');
  if (at < 0) return null;
  for (let i = at + 1; i < entries.length; i++) {
    const kind = entries[i]!.kind.type;
    if (kind === 'Message' || kind === 'RoomEvent') return entries[i]!.id;
  }
  return null;
}

/** The divider after the SDK's marker stands before `sdk` (`null`: no marker). */
export function nextMarker(state: MarkerState, sdk: string | null): MarkerState {
  if (state.hidden) return state;
  const shown = state.shown;
  if (shown === null) return sdk === null ? state : { shown: { boundary: sdk, fading: false }, hidden: false };
  if (shown.fading || sdk === shown.boundary) return state;
  // Receipts moved or removed the marker: the divider stays where it was and fades.
  return { ...state, shown: { ...shown, fading: true } };
}

/** The fade ended: the divider goes, and no later one shows while the timeline is on screen. */
export function markerFaded(state: MarkerState): MarkerState {
  return state.shown?.fading ? { shown: null, hidden: true } : state;
}

/**
 * Whether the reader has seen where the unread messages begin: the SDK's marker is not in the
 * loaded entries (`sdk` null), or the "New" divider was on screen while it stood before `sdk`
 * (`seen`). Until then the cross-device fully-read marker stays (the desktop's
 * `has_unseen_read_marker`).
 */
export function markerAcknowledged(sdk: string | null, seen: string | null): boolean {
  return sdk === null || sdk === seen;
}
