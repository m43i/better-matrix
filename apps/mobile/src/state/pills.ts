// Pills in the composer: what a picked suggestion inserts that isn't just text, as the engine's
// `html.rs` names them. The phone's text field holds plain text, so a pill is its label in the
// draft (`@Ada Lovelace`, `#design`, `@room`, `:blobcat:`) plus an entry in the composer's
// `pills`, which says what the label stands for. On send each label still in the draft (whole,
// not inside a longer word) goes as Matrix HTML (`format.ts`) and its people as `m.mentions`; a
// label the user broke by editing it is plain text again. Pill ids:
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

/** A pill found in a draft: where it stands. */
export interface Placed {
  pill: Pill;
  start: number;
  end: number;
}

const WORD = /[\p{L}\p{N}_]/u;

/**
 * Where `pills` stand in `draft`, in order: each pill names one place where its label stands
 * whole (not inside a longer word), the earliest not taken by another; longer labels first where
 * two start at the same place.
 */
export function placePills(draft: string, pills: readonly Pill[]): Placed[] {
  if (pills.length === 0) return [];
  const left = new Map<string, Pill[]>();

  for (const pill of pills) left.set(pill.label, [...(left.get(pill.label) ?? []), pill]);

  const labels = [...left.keys()]
    .filter((label) => label !== '')
    .sort((a, b) => b.length - a.length);

  const placed: Placed[] = [];
  let at = 0;

  while (at < draft.length) {
    const before = draft[at - 1];

    const label =
      before !== undefined && WORD.test(before) && WORD.test(draft[at] ?? '')
        ? undefined
        : labels.find((candidate) => {
            if (!draft.startsWith(candidate, at) || (left.get(candidate)?.length ?? 0) === 0)
              return false;
            const after = draft[at + candidate.length];

            return after === undefined || !WORD.test(after) || !WORD.test(candidate.slice(-1));
          });

    if (label === undefined) {
      at += 1;
      continue;
    }

    const pill = left.get(label)?.shift();

    if (pill) placed.push({ pill, start: at, end: at + label.length });
    at += label.length;
  }

  return placed;
}

/** The pills still standing in `draft`, in their order there; `pills` itself when all stand. */
export function livePills(draft: string, pills: readonly Pill[]): Pill[] {
  const placed = placePills(draft, pills);

  return placed.length === pills.length ? [...pills] : placed.map((p) => p.pill);
}

/** Who the pills standing in a draft mention. */
export function mentionsOf(draft: string, pills: readonly Pill[]): Mentions {
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

function escapeHtml(text: string): string {
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
