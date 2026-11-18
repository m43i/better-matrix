// A map keeping only its most recently used entries: for caches that would otherwise grow with
// everything the app has shown.

export class Lru<K, V> {
  private readonly entries = new Map<K, V>();

  private readonly limit: number;

  constructor(limit: number) {
    this.limit = limit;
  }

  /** The value kept for `key`, which becomes the most recently used. */
  get(key: K): V | undefined {
    const value = this.entries.get(key);

    if (value !== undefined) {
      this.entries.delete(key);
      this.entries.set(key, value);
    }

    return value;
  }

  /** Keeps `value` for `key`, letting the least recently used entry go past the limit. */
  set(key: K, value: V): void {
    this.entries.delete(key);
    this.entries.set(key, value);

    if (this.entries.size > this.limit) {
      const oldest = this.entries.keys().next();

      if (!oldest.done) this.entries.delete(oldest.value);
    }
  }
}
