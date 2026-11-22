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

const WORD = /[\p{L}\p{N}_]/u;

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

    const joined =
      (before !== undefined && WORD.test(before) && WORD.test(pill.label[0] ?? '')) ||
      (after !== undefined && WORD.test(after) && WORD.test(pill.label.slice(-1)));

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
 * `pills` after the draft's `[start, end)` was replaced by `length` characters: those before
 * it stay, those after it move, and those it touched inside are gone.
 */
export function editPills(
  pills: readonly DraftPill[],
  start: number,
  end: number,
  length: number,
): DraftPill[] {
  const shift = length - (end - start);

  return pills.flatMap((pill) => {
    if (pill.start + pill.label.length <= start) return [pill];

    if (pill.start >= end) return [{ ...pill, start: pill.start + shift }];

    return [];
  });
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

  const moved = editPills(pills, prefix, before.length - suffix, after.length - suffix - prefix);

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

/** The words a pill's link shows and the plain body says: a person's name without the `@`. */
export function pillText(pill: Pill): string {
  return pill.id.startsWith('user:') ? pill.label.replace(/^@/, '') : pill.label;
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
