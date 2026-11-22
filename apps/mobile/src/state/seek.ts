// A timeline's way to a message: the one a link opened it at (its `target`), then any a quote's
// tap looks for. Read marking waits for the opening only: until the timeline got to its target,
// gave up looking, or the reader moved on with a quote, the newest isn't marked read on the way.
// A quote's own search never holds read marking back: the reader is wherever it scrolls them,
// and the newest is marked read once they are back at it.

export interface Seek {
  /** The link's message the timeline opened at. */
  target: string | null;
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

/** A timeline opened at `target`, or at the newest when there is none. */
export function openSeek(target: string | null): Seek {
  return { target, positioned: target === null, quote: null, quoteHere: false };
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
 * The engine said the quote `eventId` shows in this timeline, after its search ran out of pages
 * (and ended): it looks again, paging on until it finds it or history starts. Only while no other
 * quote is being looked for; the caller asks only while the quote's jump is still the latest.
 */
export function quoteHere(seek: Seek, eventId: string): Seek {
  if (seek.quote !== null && seek.quote !== eventId) return seek;

  return { ...seek, positioned: true, quote: eventId, quoteHere: true };
}

/** What a search for a message does next. */
export type SeekStep =
  /** Its jump is no longer the latest, or its screen left: stop, quietly. */
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
  /** Whether the jump that began it is still wanted. */
  wanted: boolean;
  /** Whether the message is among the loaded rows. */
  loaded: boolean;
  /** Whether the timeline is opening, loading a page, or stopped by a paging failure. */
  busy: boolean;
  /** Whether history starts in the loaded rows. */
  atStart: boolean;
  /** Pages loaded so far looking for it. */
  pages: number;
  /** Pages to load before asking elsewhere. */
  budget: number;
  /** Page until history starts: a hit known to be local, or a quote the engine places here. */
  unbounded: boolean;
}

/** The next step of a search for a message. */
export function seekStep(progress: SeekProgress): SeekStep {
  if (progress.quote && !progress.wanted) return 'stop';

  if (progress.loaded) return 'scroll';

  if (progress.busy) return 'wait';

  if (!progress.atStart && (progress.unbounded || progress.pages < progress.budget)) return 'page';

  return progress.quote && progress.unbounded ? 'missing' : 'elsewhere';
}
