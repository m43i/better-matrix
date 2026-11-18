// Failed fetches and when each may be tried again: soon after the first failure, then less and
// less often, so a server hiccup heals by itself and media that is gone costs little. The
// desktop's `state::retry::Retries`, with times in ms since the epoch.

/** The wait after the first failure in a row. */
export const FIRST_WAIT_MS = 5_000;

/** The longest wait between tries. */
export const LONGEST_WAIT_MS = 5 * 60_000;

/** The wait after the `attempts`th failure in a row: 5 s, doubling, up to 5 minutes. */
export function retryWait(attempts: number): number {
  const doublings = Math.min(Math.max(attempts - 1, 0), 16);

  return Math.min(FIRST_WAIT_MS * 2 ** doublings, LONGEST_WAIT_MS);
}

export class Retries<K> {
  /** How often each failed in a row, and when it may be tried again. */
  private readonly failures = new Map<K, { attempts: number; at: number }>();

  failed(key: K, now: number): void {
    const attempts = (this.failures.get(key)?.attempts ?? 0) + 1;
    this.failures.set(key, { attempts, at: now + retryWait(attempts) });
  }

  succeeded(key: K): void {
    this.failures.delete(key);
  }

  /** Whether `key` failed the last time it was tried. */
  hasFailed(key: K): boolean {
    return this.failures.has(key);
  }

  /** Whether `key` may be fetched at `now`: it never failed, or its wait is over. */
  mayTry(key: K, now: number): boolean {
    const at = this.failures.get(key)?.at;

    return at === undefined || at <= now;
  }

  /** When `key`'s wait ends; `undefined` when it never failed. */
  waitEnds(key: K): number | undefined {
    return this.failures.get(key)?.at;
  }

  /** Forgets all but the `most` failures that may be tried again last. */
  keepLatest(most: number): void {
    if (this.failures.size <= most) return;

    const byWait = [...this.failures].sort(([, a], [, b]) => a.at - b.at);

    for (const [key] of byWait.slice(0, byWait.length - Math.max(most, 0))) {
      this.failures.delete(key);
    }
  }
}
