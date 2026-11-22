// A timeline's way to a message: the one a link opened it at (its `target`), then any a quote's
// tap looks for. Read marking waits for the opening only: until the timeline got to its target,
// gave up looking, or the reader moved on with a quote, the newest isn't marked read on the way.
// A quote's own search never holds read marking back: the reader is wherever it scrolls them,
// and the newest is marked read once they are back at it. How far a search pages back is its
// `Reach`: a few pages, on to the start of history for a search hit known to be local, and on to
// the start for a message the engine located in this timeline, which is then said to be gone.

/**
 * How far a search for a message pages back. `near`: a few pages, then it ends (a quote then
 * asks the engine where it is). `local`: a search hit in local history, paged for until history
 * starts, then it ends quietly. `located`: the engine said it shows in this timeline
 * (`LocateMessage`), paged for until history starts, which then says it is gone.
 */
export type Reach = 'near' | 'local' | 'located';

/** A screen's `reach` route parameter as a `Reach`: `near` when absent or unknown. */
export function reachParam(param: string | undefined): Reach {
  return param === 'local' || param === 'located' ? param : 'near';
}

export interface Seek {
  /** The link's message the timeline opened at. */
  target: string | null;
  /** How far the search for the target pages back. */
  targetReach: Reach;
  /** Whether the opening is over: at the target, given up, or left for a quote. */
  positioned: boolean;
  /** The message a quote's tap is looking for, until found or given up. */
  quote: string | null;
  /**
   * Whether the engine said the quote shows in this timeline once paging back a few pages
   * didn't find it: the search then pages on, to the start of history if need be.
   */
  quoteHere: boolean;
}

/** A timeline opened at `target`, looked for as far as `reach` says, or at the newest. */
export function openSeek(target: string | null, reach: Reach = 'near'): Seek {
  return { target, targetReach: reach, positioned: target === null, quote: null, quoteHere: false };
}

/** How far the search for the message being looked for now (`sought`) pages back. */
export function soughtReach(seek: Seek): Reach {
  if (seek.quote !== null) return seek.quoteHere ? 'located' : 'near';

  return seek.targetReach;
}

/** The message the timeline is looking for now: a quote's, else the target until positioned. */
export function sought(seek: Seek): string | null {
  return seek.quote ?? (seek.positioned ? null : seek.target);
}

/**
 * A quote tapped: the timeline looks for `missing`, the quoted message it hasn't loaded, or
 * nothing when it scrolled to it at once. Either way the opening is over and any earlier quote's
 * search too.
 */
export function quoteTapped(seek: Seek, missing: string | null): Seek {
  return { ...seek, positioned: true, quote: missing, quoteHere: false };
}

/** The search for `eventId` ended: the timeline got there, or gave up looking. */
export function seekEnded(seek: Seek, eventId: string): Seek {
  if (seek.quote !== null)
    return seek.quote === eventId ? { ...seek, quote: null, quoteHere: false } : seek;

  if (seek.target === eventId && !seek.positioned) return { ...seek, positioned: true };

  return seek;
}

/**
 * The engine said the quote `eventId` shows in this timeline as `located` (the message an edit
 * edits; else `eventId` itself), after its search ran out of pages (and ended): it looks for
 * `located`, scrolling to it once loaded and paging on until it finds it or history starts. Only
 * while no other quote is being looked for; the caller asks only while the quote's jump is still
 * the latest.
 */
export function quoteHere(seek: Seek, eventId: string, located: string = eventId): Seek {
  if (seek.quote !== null && seek.quote !== eventId && seek.quote !== located) return seek;

  return { ...seek, positioned: true, quote: located, quoteHere: true };
}

/** What a search for a message does next. */
export type SeekStep =
  /**
   * Its jump (a quote's, or the one that opened a located target) is no longer the latest, or
   * its screen left: stop, quietly.
   */
  | 'stop'
  /** It is loaded: scroll to it. */
  | 'scroll'
  /** The timeline is opening, a page is coming, or paging failed (until Try again): wait. */
  | 'wait'
  /** Load an older page. */
  | 'page'
  /** Not found here: ask the engine where a quote shows (a link's search just ends). */
  | 'elsewhere'
  /** History started without it, though the engine said it shows here: it is gone. */
  | 'missing';

/** Where a search for a message stands, as `seekStep` reads it. */
export interface SeekProgress {
  /** Whether this is a quote's search (else a link's target). */
  quote: boolean;
  /** Whether the jump that began it is still wanted; a target opened without one always is. */
  wanted: boolean;
  /** Whether the message is among the loaded rows. */
  loaded: boolean;
  /** Whether the timeline is opening, loading a page, or stopped by a paging failure. */
  busy: boolean;
  /** Whether history starts in the loaded rows. */
  atStart: boolean;
  /** Pages loaded so far looking for it. */
  pages: number;
  /** Pages to load before a `near` search ends. */
  budget: number;
  /** How far it pages back (`soughtReach`). */
  reach: Reach;
}

/** The next step of a search for a message. */
export function seekStep(progress: SeekProgress): SeekStep {
  if (!progress.wanted) return 'stop';

  if (progress.loaded) return 'scroll';

  if (progress.busy) return 'wait';

  const bounded = progress.reach === 'near';

  if (!progress.atStart && (!bounded || progress.pages < progress.budget)) return 'page';

  return progress.reach === 'located' ? 'missing' : 'elsewhere';
}
