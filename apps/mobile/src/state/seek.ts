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
}

/** A timeline opened at `target`, or at the newest when there is none. */
export function openSeek(target: string | null): Seek {
  return { target, positioned: target === null, quote: null };
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
  return { ...seek, positioned: true, quote: missing };
}

/** The search for `eventId` ended: the timeline got there, or gave up looking. */
export function seekEnded(seek: Seek, eventId: string): Seek {
  if (seek.quote !== null) return seek.quote === eventId ? { ...seek, quote: null } : seek;

  if (seek.target === eventId && !seek.positioned) return { ...seek, positioned: true };

  return seek;
}
