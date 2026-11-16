// The emoji the picker offers: every Unicode emoji with its English name, tags and shortcodes
// (emojibase's compact data, with its own and GitHub's shortcodes, as the desktop's `emojis`
// crate has gemoji's), in Unicode's groups and order. Search and recents are pure.

import { z } from 'zod';
import compact from 'emojibase-data/en/compact.json';
import emojibaseCodes from 'emojibase-data/en/shortcodes/emojibase.json';
import githubCodes from 'emojibase-data/en/shortcodes/github.json';

/** How many search results show at most, as on the desktop. */
export const RESULTS = 96;

/** How many recently used emoji the picker keeps. */
export const RECENT_KEPT = 24;

/** The reactions most messages get, shown before anything was used (the desktop's `QUICK`). */
export const QUICK = ['👍', '❤️', '😂', '🎉', '😮', '😢', '🙏', '👀'] as const;

/** The picker's groups, in Unicode's order (skin tone components are left out). */
export const GROUPS = [
  'smileys',
  'people',
  'nature',
  'food',
  'travel',
  'activities',
  'objects',
  'symbols',
  'flags',
] as const;

export type Group = (typeof GROUPS)[number];

export interface Emoji {
  emoji: string;
  /** The English name, as the emoji's label. */
  label: string;
  shortcodes: string[];
  group: Group;
}

const compactEmoji = z.object({
  hexcode: z.string(),
  label: z.string(),
  unicode: z.string(),
  group: z.number().optional(),
  order: z.number().optional(),
  tags: z.array(z.string()).optional(),
});

type Shortcodes = Record<string, string | string[]>;

/** Unicode's group numbers (emojibase's `meta/groups.json`) by picker group; 2 is components. */
const GROUP_BY_NUMBER = new Map<number, Group>([
  [0, 'smileys'],
  [1, 'people'],
  [3, 'nature'],
  [4, 'food'],
  [5, 'travel'],
  [6, 'activities'],
  [7, 'objects'],
  [8, 'symbols'],
  [9, 'flags'],
]);

interface Indexed extends Emoji {
  /** Name, tags and shortcodes, lower case, for search. */
  words: string[];
}

let index: Indexed[] | null = null;

function codesOf(table: Shortcodes, hexcode: string): string[] {
  const codes = table[hexcode];

  return codes === undefined ? [] : Array.isArray(codes) ? codes : [codes];
}

/** Every emoji in the picker, built once on first use. */
function all(): Indexed[] {
  if (index) return index;
  const data = z.array(compactEmoji).parse(compact);
  index = data
    .filter((e) => e.group !== undefined && GROUP_BY_NUMBER.get(e.group) !== undefined)
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
    .map((e) => {
      const shortcodes = [
        ...new Set([...codesOf(githubCodes, e.hexcode), ...codesOf(emojibaseCodes, e.hexcode)]),
      ];

      return {
        emoji: e.unicode,
        label: e.label,
        shortcodes,
        group: GROUP_BY_NUMBER.get(e.group ?? -1) ?? 'symbols',
        words: [
          e.label.toLowerCase(),
          ...(e.tags ?? []).map((t) => t.toLowerCase()),
          ...shortcodes,
        ],
      };
    });

  return index;
}

/** The emoji of one group, in Unicode's order. */
export function emojiIn(group: Group): Emoji[] {
  return all().filter((e) => e.group === group);
}

/** The emoji by group, in the picker's order. */
export function emojiByGroup(): Array<{ group: Group; emoji: Emoji[] }> {
  const groups = new Map<Group, Emoji[]>(GROUPS.map((g) => [g, []]));

  for (const e of all()) groups.get(e.group)?.push(e);

  return GROUPS.map((group) => ({ group, emoji: groups.get(group) ?? [] }));
}

/**
 * The emoji whose name, a tag or a shortcode holds every word of `query` (colons ignored, so
 * `:tada:` finds 🎉), up to `RESULTS`. An exact shortcode comes first, then names and shortcodes
 * starting with the query, then the rest in Unicode's order.
 */
export function searchEmoji(query: string): Emoji[] {
  const words = query.toLowerCase().replace(/:/g, ' ').split(/\s+/).filter(Boolean);

  if (words.length === 0) return [];
  const whole = words.join('_');

  const rank = (e: Indexed): number => {
    if (e.shortcodes.includes(whole)) return 0;

    if (
      e.label.toLowerCase().startsWith(words.join(' ')) ||
      e.shortcodes.some((c) => c.startsWith(whole))
    )
      return 1;

    return 2;
  };

  return all()
    .filter((e) => words.every((word) => e.words.some((w) => w.includes(word))))
    .map((e, position) => ({ e, position, rank: rank(e) }))
    .sort((a, b) => a.rank - b.rank || a.position - b.position)
    .slice(0, RESULTS)
    .map(({ e }) => ({ emoji: e.emoji, label: e.label, shortcodes: e.shortcodes, group: e.group }));
}

/** The name an emoji is announced by, when it is in the data. */
export function emojiLabel(emoji: string): string | undefined {
  return all().find((e) => e.emoji === emoji)?.label;
}

/** `recent` with `emoji` used just now: first, once, at most `RECENT_KEPT`. */
export function rememberEmoji(recent: readonly string[], emoji: string): string[] {
  return [emoji, ...recent.filter((e) => e !== emoji)].slice(0, RECENT_KEPT);
}
