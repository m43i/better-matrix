// The time lists are drawn at, moving on once a minute: rows show "today" and "yesterday" right
// without redrawing on every store change. One clock serves every view (each date divider, thread
// link and list), ticking on the minute while any of them shows it.

import { useSyncExternalStore } from 'react';

const MINUTE_MS = 60_000;

export interface MinuteClock {
  subscribe: (listener: () => void) => () => void;
  /** The time it shows: when it last ticked, within the current minute while it is followed. */
  snapshot: () => number;
}

/** A clock telling its listeners at each minute boundary of `clock`; it stops when none is left. */
export function minuteClock(clock: () => number = Date.now): MinuteClock {
  let now = clock();
  const listeners = new Set<() => void>();
  let timer: ReturnType<typeof setTimeout> | null = null;

  const schedule = () => {
    timer = setTimeout(
      () => {
        now = clock();

        for (const listener of listeners) listener();
        schedule();
      },
      MINUTE_MS - (clock() % MINUTE_MS),
    );
  };

  return {
    subscribe(listener) {
      listeners.add(listener);

      if (timer === null) {
        // Left alone across a minute: it shows this one again (views mounted meanwhile redraw).
        const at = clock();

        if (Math.floor(at / MINUTE_MS) !== Math.floor(now / MINUTE_MS)) now = at;
        schedule();
      }

      return () => {
        listeners.delete(listener);

        if (listeners.size === 0 && timer !== null) {
          clearTimeout(timer);
          timer = null;
        }
      };
    },
    snapshot: () => now,
  };
}

const shared = minuteClock();

export function useNow(): number {
  return useSyncExternalStore(shared.subscribe, shared.snapshot, shared.snapshot);
}
