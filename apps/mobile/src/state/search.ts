// Fuzzy matching shared with matrix-core::search; positions count Unicode scalar values.

export interface FuzzyMatch {
  positions: number[];
  rank: number;
}

export function fuzzyMatch(text: string, query: string): FuzzyMatch | null {
  const chars = Array.from(text);
  const folded = chars.flatMap((c, i) => Array.from(c.toLowerCase(), (c) => ({ c, i })));
  const occurrences = new Map<string, number[]>();
  folded.forEach(({ c }, i) => {
    const indices = occurrences.get(c) ?? [];
    indices.push(i);
    occurrences.set(c, indices);
  });
  const positions = new Set<number>();
  let rank = 0;

  for (const term of query.trim().split(/\s+/).filter(Boolean)) {
    const needle = Array.from(term.toLowerCase());
    let best: FuzzyMatch | null = null;

    if (needle.some((c) => !occurrences.has(c))) return null;

    for (const start of occurrences.get(needle[0] ?? '') ?? []) {
      let at = start;
      const found = [folded[start]?.i ?? 0];

      for (const c of needle.slice(1)) {
        const candidates = occurrences.get(c) ?? [];
        let low = 0;
        let high = candidates.length;

        while (low < high) {
          const middle = Math.floor((low + high) / 2);

          if ((candidates[middle] ?? 0) <= at) low = middle + 1;
          else high = middle;
        }

        const index = candidates[low];

        if (index === undefined) break;
        at = index;
        found.push(folded[at]?.i ?? 0);
      }

      if (found.length !== needle.length) continue;
      const first = found[0] ?? 0;
      const last = found[found.length - 1] ?? first;
      const gaps = Math.max(0, last - first + 1 - needle.length);
      const boundary = first === 0 || !/[\p{L}\p{N}]/u.test(chars[first - 1] ?? '');
      const score = gaps * 100 + (boundary ? 0 : 10) + first;

      if (best === null || score < best.rank) best = { positions: found, rank: score };

      if (gaps === 0 && boundary) break;
    }

    if (best === null) return null;
    rank += best.rank;
    best.positions.forEach((i) => positions.add(i));
  }

  return { positions: [...positions].sort((a, b) => a - b), rank };
}
