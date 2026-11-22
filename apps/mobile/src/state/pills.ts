// Pills in the composer: what a picked suggestion inserts that isn't just text, as the engine's
// `html.rs` names them. The phone's text field holds plain text, so a pill is its label in the
// draft (`@Ada Lovelace`, `#design`, `@room`, `:blobcat:`) plus an entry in the composer's
// `pills`, which says what the label stands for and where it stands (`DraftPill`). A pill is
// bound to its place, not its label: the same words typed elsewhere stay text, and two people
// with one name keep their own IDs. Each edit of the draft moves the pills after it
// (`trackPills`); a pill whose label the user broke by editing it, or that now sits inside a
// longer word, is plain text again. On send each pill still standing goes as Matrix HTML
// (`format.ts`) and its people as `m.mentions`. Pill ids:
//
// - `user:@ada:example.org`, `room:!id:example.org` (or `room:#alias:example.org`): a mention.
// - `everyone`: `@room`, which goes as plain text and mentions the room.
// - `emote:mxc://…`: a custom emoji, its label the short code.
// - `spoiler`: hidden words, its label the words (line breaks included). Only an edit makes one
//   (`editDraft`, as the engine's `to_pills` does): the spoiler's words stand in the draft
//   whatever they say, so nothing in them reads as a marker that could end the spoiler or show
//   it. Unlike the others it is a range the user may edit: words typed or deleted inside it stay
//   hidden, and only deleting all of it ends it (`editPills`).

import type { Mentions } from '@better-matrix/matrix-native/types';

export interface Pill {
  id: string;
  /** What the draft shows for it. */
  label: string;
}

/** A pill in a draft: its label stands from `start` (UTF-16 offsets, as the field counts). */
export interface DraftPill extends Pill {
  start: number;
}

export const EVERYONE: Pill = { id: 'everyone', label: '@room' };

/** A mention of a person, shown by their name. */
export function userPill(userId: string, name: string): Pill {
  return { id: `user:${userId}`, label: `@${name.replace(/^@/, '') || userId}` };
}

/** A mention of a room, shown by its name. */
export function roomPill(roomId: string, name: string): Pill {
  return { id: `room:${roomId}`, label: name.startsWith('#') ? name : `#${name}` };
}

/** A custom emoji, shown by its short code. */
export function emotePill(url: string, shortcode: string): Pill {
  return { id: `emote:${url}`, label: `:${shortcode}:` };
}

/** The id of a spoiler's pill. */
export const SPOILER = 'spoiler';

/** Words hidden until tapped: `[Spoiler]` in the plain body. */
export function spoilerPill(words: string): Pill {
  return { id: SPOILER, label: words };
}

export function isSpoiler(pill: Pill): boolean {
  return pill.id === SPOILER;
}

/** `pill` standing at `start` in a draft. */
export function pillAt(pill: Pill, start: number): DraftPill {
  return { id: pill.id, label: pill.label, start };
}

/** A pill found in a draft: where it stands. */
export interface Placed {
  pill: DraftPill;
  start: number;
  end: number;
}

/** What makes a longer word of a pill's label: not `_`, italic's marker, which may wrap one. */
const WORD = /[\p{L}\p{N}]/u;

/**
 * The pills standing in `draft`, in order: each whose label is still whole at its place, not
 * inside a longer word, and not overlapping one before it.
 */
export function placePills(draft: string, pills: readonly DraftPill[]): Placed[] {
  const placed: Placed[] = [];
  let taken = 0;

  for (const pill of [...pills].sort((a, b) => a.start - b.start)) {
    const start = pill.start;
    const end = start + pill.label.length;

    if (pill.label === '' || start < taken || !draft.startsWith(pill.label, start)) continue;
    const before = draft[start - 1];
    const after = draft[end];

    // A spoiler's words stay hidden whatever stands next to them.
    const joined =
      !isSpoiler(pill) &&
      ((before !== undefined && WORD.test(before) && WORD.test(pill.label[0] ?? '')) ||
        (after !== undefined && WORD.test(after) && WORD.test(pill.label.slice(-1))));

    if (joined) continue;
    placed.push({ pill, start, end });
    taken = end;
  }

  return placed;
}

/** The pills still standing in `draft`, in their order there. */
export function livePills(draft: string, pills: readonly DraftPill[]): DraftPill[] {
  return placePills(draft, pills).map((p) => p.pill);
}

/**
 * `pills` after the draft's `[start, end)` was replaced by `inserted`: those before it stay,
 * those after it move, and those it touched inside are gone, except a spoiler, which keeps what
 * is left of its words and takes in what was typed inside it (not what was typed at either of
 * its ends); a spoiler is gone only once the edit took all of its words.
 */
export function editPills(
  pills: readonly DraftPill[],
  start: number,
  end: number,
  inserted: string,
): DraftPill[] {
  const shift = inserted.length - (end - start);

  return pills.flatMap((pill) => {
    if (pill.start + pill.label.length <= start) return [pill];

    if (pill.start >= end) return [{ ...pill, start: pill.start + shift }];

    return isSpoiler(pill) ? editSpoiler(pill, start, end, inserted) : [];
  });
}

/** A spoiler pill the edit of `[start, end)` into `inserted` overlaps. */
function editSpoiler(pill: DraftPill, start: number, end: number, inserted: string): DraftPill[] {
  const pillEnd = pill.start + pill.label.length;
  const from = Math.max(start, pill.start) - pill.start;
  const to = Math.min(end, pillEnd) - pill.start;
  const inside = start >= pill.start && end <= pillEnd;

  const label = inside
    ? pill.label.slice(0, from) + inserted + pill.label.slice(to)
    : pill.label.slice(0, from) + pill.label.slice(to);

  if (label === '') return [];

  // Cut at its start, the words left begin after what was put in the cut's place.
  const at = start < pill.start ? start + inserted.length : pill.start;

  return [{ ...pill, label, start: at }];
}

/**
 * A change of a draft whose place is known (the toolbar's markers, a list continued): `remove`
 * characters from `at` replaced by `insert`. A list of them applies in order, each to the text
 * the one before it left.
 */
export interface Splice {
  at: number;
  remove: number;
  insert: string;
}

/** `text` with `splices` applied in order. */
export function applySplices(text: string, splices: readonly Splice[]): string {
  return splices.reduce(
    (out, { at, remove, insert }) => out.slice(0, at) + insert + out.slice(at + remove),
    text,
  );
}

/**
 * `pills` moved by `splices`, each exactly where it was made: a marker put right before or after
 * a pill moves it or leaves it, never takes it with it (as a diff of the two texts would, seeing
 * one change across the pill). A splice inside a pill's label still breaks it.
 */
export function splicePills(pills: readonly DraftPill[], splices: readonly Splice[]): DraftPill[] {
  return splices.reduce<DraftPill[]>(
    (moved, { at, remove, insert }) => editPills(moved, at, at + remove, insert),
    [...pills],
  );
}

/**
 * The pills of `before` carried into `after`, the same draft edited by the user: the edit is
 * what lies between their common start and end. Where it could lie in more than one place (a
 * character typed or deleted in repeated text: `@Ada @Ada` to `@Ada`), `caret` (where the
 * field's caret is) says which. The pills still standing in `after`.
 */
export function trackPills(
  before: string,
  after: string,
  pills: readonly DraftPill[],
  caret?: number,
): DraftPill[] {
  if (pills.length === 0 || before === after) return livePills(after, pills);
  const shortest = Math.min(before.length, after.length);
  let prefix = 0;

  while (prefix < shortest && before[prefix] === after[prefix]) prefix += 1;
  let suffix = 0;

  while (
    suffix < shortest &&
    before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
  )
    suffix += 1;
  // How far the edit could slide in repeated text; the caret picks where it was.
  const slide = Math.max(0, prefix + suffix - shortest);

  if (slide > 0 && caret !== undefined) prefix = Math.min(prefix, Math.max(prefix - slide, caret));
  suffix = Math.min(suffix, shortest - prefix);

  const moved = editPills(
    pills,
    prefix,
    before.length - suffix,
    after.slice(prefix, after.length - suffix),
  );

  return livePills(after, moved);
}

/** The pills within `[from, to)` of a draft, placed in that slice of it, moved by `offset`. */
export function slicePills(
  pills: readonly DraftPill[],
  from: number,
  to: number,
  offset = 0,
): DraftPill[] {
  return pills.flatMap((pill) =>
    pill.start >= from && pill.start + pill.label.length <= to
      ? [{ ...pill, start: pill.start - from + offset }]
      : [],
  );
}

/** Who the pills standing in a draft mention. */
export function mentionsOf(draft: string, pills: readonly DraftPill[]): Mentions {
  const users: string[] = [];
  let room = false;

  for (const { pill } of placePills(draft, pills)) {
    if (pill.id === EVERYONE.id) room = true;

    if (pill.id.startsWith('user:')) {
      const id = pill.id.slice('user:'.length);

      if (!users.includes(id)) users.push(id);
    }
  }

  return { user_ids: users, room };
}

/**
 * What the plain body says for a pill: a person's name without the `@` (the words its link
 * shows), a spoiler `[Spoiler]`.
 */
export function pillText(pill: Pill): string {
  if (isSpoiler(pill)) return SPOILER_FALLBACK;

  return pill.id.startsWith('user:') ? pill.label.replace(/^@/, '') : pill.label;
}

/** What a spoiler's words are in a plain body: the same in every language, as the desktop's. */
export const SPOILER_FALLBACK = '[Spoiler]';

/** A spoiler's words as the HTML inside its `<span data-mx-spoiler>`: line breaks as `<br />`. */
export function spoilerHtml(words: string): string {
  return escapeHtml(words).replace(/\r?\n/g, '<br />');
}

/** Whether a pill goes as HTML (everything but `@room`, which is plain text). */
export function pillFormats(pill: Pill): boolean {
  return pill.id !== EVERYONE.id;
}

/** `text` with HTML's special characters escaped, as text inside an element or attribute. */
export function escapeHtml(text: string): string {
  return text.replace(/[&<>"]/g, (c) =>
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;',
  );
}

/** A pill as Matrix HTML, as the engine's `from_pills` writes it. */
export function pillHtml(pill: Pill): string {
  const mention = /^(user|room):(.+)$/.exec(pill.id);

  if (mention)
    return `<a href="${escapeHtml(`https://matrix.to/#/${mention[2]}`)}">${escapeHtml(pillText(pill))}</a>`;

  if (isSpoiler(pill)) return `<span data-mx-spoiler>${spoilerHtml(pill.label)}</span>`;

  if (pill.id.startsWith('emote:')) {
    const label = escapeHtml(pill.label);

    return `<img data-mx-emoticon src="${escapeHtml(pill.id.slice('emote:'.length))}" alt="${label}" title="${label}" height="32" />`;
  }

  return escapeHtml(pill.label);
}

/** The text before the caret that a suggestion would replace, and what kind of thing it asks for. */
export interface Trigger {
  kind: 'user' | 'room' | 'emoji' | 'command';
  /** What was typed after the sign. */
  query: string;
  /** Where the sign stands; the replaced text runs from here to `end`. */
  start: number;
  end: number;
}

const SIGNS = { '@': 'user', '#': 'room', ':': 'emoji' } as const;

/**
 * What the word the caret ends asks to complete: `@name` (people), `#name` (rooms), `:code`
 * (emoji, from two characters, as the desktop asks) or a command at the very start (`/me`). A
 * sign counts at the start of a word only, so `a@b` and `12:30` ask for nothing.
 */
export function triggerAt(draft: string, caret: number): Trigger | null {
  const before = draft.slice(0, caret);
  const command = /^\/([a-z]*)$/i.exec(before);

  if (command) return { kind: 'command', query: command[1] ?? '', start: 0, end: caret };
  const match = /(^|[\s([{"'])([@#:])([^\s@#:]*)$/u.exec(before);

  if (!match) return null;
  // SAFETY: the pattern's second group is one of SIGNS' keys.
  const sign = match[2] as keyof typeof SIGNS;
  const query = match[3] ?? '';

  if (sign === ':' && query.length < 2) return null;

  return { kind: SIGNS[sign], query, start: caret - query.length - 1, end: caret };
}

/** `draft` with a trigger's text replaced by `insert` and a space; the caret goes after it. */
export function completeAt(draft: string, trigger: Trigger, insert: string) {
  const after = draft.slice(trigger.end);
  const spaced = after.startsWith(' ') ? insert : `${insert} `;
  const text = draft.slice(0, trigger.start) + spaced + after;
  const caret = trigger.start + spaced.length + (after.startsWith(' ') ? 1 : 0);

  return { text, selection: { start: caret, end: caret } };
}
