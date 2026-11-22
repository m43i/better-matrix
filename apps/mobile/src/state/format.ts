// Formatting in the composer. The phone's native text field holds plain text, so formatting is
// written as markers in it (`**bold**`, `_italic_`, `~~strike~~`, `` `code` ``, `[text](url)`,
// `- ` and `1. ` lists, `> ` quotes, ``` fences), which the toolbar inserts and removes; two
// more the toolbar doesn't offer are Matrix's own, `||spoiler||` and `$$maths$$`. Mentions and
// custom emoji are pills (`pills.ts`): labels at places in the text the composer knows. On send,
// `outgoingText` turns the text into what the desktop's rich field sends: the plain body, and
// Matrix HTML serialised as Rune's `TextEdit::to_html` does (apps/desktop/src/state/composer.rs),
// only when the text carries formatting. `draftOf` goes the other way, for editing a message.

import type { Mentions } from '@better-matrix/matrix-native/types';

import { mentionOf } from './html';
import {
  applySplices,
  type DraftPill,
  emotePill,
  isSpoiler,
  livePills,
  mentionsOf,
  type Pill,
  pillAt,
  pillFormats,
  pillHtml,
  pillText,
  placePills,
  roomPill,
  type Splice,
  SPOILER_FALLBACK,
  spoilerHtml,
  spoilerPill,
  userPill,
} from './pills';

/** Every formatting action the toolbar offers, in its order (the desktop's `Format::ALL`). */
export const FORMATS = [
  'bold',
  'italic',
  'strike',
  'code',
  'link',
  'bullets',
  'numbers',
  'quote',
  'codeBlock',
] as const;

export type Format = (typeof FORMATS)[number];

/** A selection in the draft, as the native text field reports it (UTF-16 offsets). */
export interface Selection {
  start: number;
  end: number;
}

/**
 * The draft after a change, where the caret or selection goes, and the exact changes made to the
 * draft before it (in order), so its pills move by them (`splicePills`) rather than by a diff.
 */
export interface Edited {
  text: string;
  selection: Selection;
  splices: Splice[];
}

/** `text` changed by `splices`, the caret or selection put at `selection`. */
function edited(text: string, splices: Splice[], selection: Selection): Edited {
  return { text: applySplices(text, splices), selection, splices };
}

const insertion = (at: number, insert: string): Splice => ({ at, remove: 0, insert });

const removal = (at: number, remove: number): Splice => ({ at, remove, insert: '' });

/** The markers of the inline formats. */
const INLINE: Partial<Record<Format, string>> = {
  bold: '**',
  italic: '_',
  strike: '~~',
  code: '`',
};

const BULLET = /^( *)[-*•] /;

const NUMBERED = /^( *)(\d+)[.)] /;

const QUOTE = /^> ?/;

const FENCE = /^ *```/;

// ----- toolbar ---------------------------------------------------------------------------------

/**
 * `text` with `format` toggled on `selection`. Inline formats wrap the selection in markers (or
 * unwrap it when it is wrapped already); with nothing selected they insert a pair and put the
 * caret between, so what is typed next is formatted. Block formats toggle a prefix on every
 * line the selection touches. A link needs its address: see `applyLink`.
 */
export function applyFormat(text: string, selection: Selection, format: Format): Edited {
  const marker = INLINE[format];

  if (marker) return toggleInline(text, ordered(selection), marker);

  switch (format) {
    case 'bullets':
    case 'numbers':
    case 'quote':
      return toggleLines(text, ordered(selection), format);
    case 'codeBlock':
      return toggleFence(text, ordered(selection));
    default:
      return { text, selection, splices: [] };
  }
}

/** `text` with the selection linked to `url`; with nothing selected, the address goes in as a link. */
export function applyLink(text: string, selection: Selection, url: string): Edited {
  const address = url.trim();
  const { start, end } = ordered(selection);

  if (!address || address === 'https://') return { text, selection, splices: [] };

  if (start === end) {
    const link = `[${address}](${address})`;
    const caret = start + link.length;

    return edited(text, [insertion(start, link)], { start: caret, end: caret });
  }

  // The selection stays where it is, between the link's brackets.
  const caret = end + 1 + `](${address})`.length;

  return edited(text, [insertion(end, `](${address})`), insertion(start, '[')], {
    start: caret,
    end: caret,
  });
}

/** The block formats on the caret's line, for the toolbar to show as on. */
export function activeBlocks(text: string, caret: number): Format[] {
  const start = lineStart(text, caret);
  const line = text.slice(start, lineEnd(text, caret));
  const active: Format[] = [];

  if (insideFence(text, start)) return ['codeBlock'];

  if (BULLET.test(line)) active.push('bullets');

  if (NUMBERED.test(line)) active.push('numbers');

  if (QUOTE.test(line)) active.push('quote');

  return active;
}

/**
 * When `next` is `previous` with a line break typed after a list item or quote, the new line
 * continues it (`- `, the next number, `> `); a break typed on an empty item ends the list
 * instead, as Slack and the desktop's editor do. `null` when the change was anything else.
 */
export function continueBlock(previous: string, next: string): Edited | null {
  if (next.length !== previous.length + 1) return null;
  let at = 0;

  while (at < previous.length && previous[at] === next[at]) at++;

  if (next[at] !== '\n' || next.slice(at + 1) !== previous.slice(at)) return null;

  if (insideFence(previous, lineStart(previous, at))) return null;
  const start = lineStart(previous, at);
  const line = previous.slice(start, lineEnd(previous, at));
  const prefix = blockPrefix(line);

  if (prefix === null) return null;

  if (line.slice(prefix.length).trim() === '') {
    // An empty item: the list ends here, and the line is left empty.
    return edited(previous, [removal(start, prefix.length)], { start, end: start });
  }

  const numbered = NUMBERED.exec(line);
  const following = numbered ? `${numbered[1]}${Number(numbered[2]) + 1}. ` : prefix;
  const caret = at + 1 + following.length;

  // The splices are from `previous`: the line break typed, and the prefix after it.
  return edited(previous, [insertion(at, `\n${following}`)], { start: caret, end: caret });
}

function blockPrefix(line: string): string | null {
  const match =
    BULLET.exec(line) ?? NUMBERED.exec(line) ?? (QUOTE.test(line) ? /^> ?/.exec(line) : null);

  if (!match) return null;

  // `>` alone continues as `> `.
  return match[0] === '>' ? '> ' : match[0];
}

function ordered({ start, end }: Selection): Selection {
  return start <= end ? { start, end } : { start: end, end: start };
}

function lineStart(text: string, at: number): number {
  return at <= 0 ? 0 : text.lastIndexOf('\n', at - 1) + 1;
}

function lineEnd(text: string, at: number): number {
  const end = text.indexOf('\n', at);

  return end < 0 ? text.length : end;
}

/** Whether the line starting at `start` lies inside a code fence. */
function insideFence(text: string, start: number): boolean {
  const before = text.slice(0, start).split('\n');

  // `before` ends with the empty piece after the last break.
  return before.slice(0, -1).filter((line) => FENCE.test(line)).length % 2 === 1;
}

function toggleInline(text: string, { start, end }: Selection, marker: string): Edited {
  const m = marker.length;

  if (start === end) {
    // Inside an empty pair: take it away again.
    if (text.slice(start - m, start) === marker && text.slice(start, start + m) === marker) {
      const at = start - m;

      return edited(text, [removal(at, 2 * m)], { start: at, end: at });
    }

    const caret = start + m;

    return edited(text, [insertion(start, marker + marker)], { start: caret, end: caret });
  }

  // Selecting a word often takes the space after it: the markers go around the word.
  let s = start;
  let e = end;

  while (s < e && /\s/.test(text[s] ?? '')) s++;

  while (e > s && /\s/.test(text[e - 1] ?? '')) e--;

  if (s === e) return { text, selection: { start, end }, splices: [] };

  // Each pair of splices goes from the end, so the first's place still holds for the second.
  if (text.slice(s - m, s) === marker && text.slice(e, e + m) === marker)
    return edited(text, [removal(e, m), removal(s - m, m)], { start: s - m, end: e - m });

  const inner = text.slice(s, e);

  if (inner.length > 2 * m && inner.startsWith(marker) && inner.endsWith(marker))
    return edited(text, [removal(e - m, m), removal(s, m)], { start: s, end: e - 2 * m });

  return edited(text, [insertion(e, marker), insertion(s, marker)], { start: s + m, end: e + m });
}

function toggleLines(
  text: string,
  { start, end }: Selection,
  format: 'bullets' | 'numbers' | 'quote',
): Edited {
  const from = lineStart(text, start);
  const to = lineEnd(text, end);
  const lines = text.slice(from, to).split('\n');

  const has = (line: string) =>
    format === 'bullets'
      ? BULLET.test(line)
      : format === 'numbers'
        ? NUMBERED.test(line)
        : QUOTE.test(line);

  const off = lines.every(has);

  // Each line's prefix is replaced: what follows it (its text) is kept as it was.
  const prefixes = lines.map((line, index) => {
    const bare = stripBlock(line);
    const kept = off || format === 'quote' ? bare.text : bare.text.trimStart();

    const prefix = off
      ? ''
      : format === 'bullets'
        ? `${bare.indent}- `
        : format === 'numbers'
          ? `${bare.indent}${index + 1}. `
          : '> ';

    return { old: line.length - kept.length, prefix };
  });

  const splices: Splice[] = [];
  let lineAt = from;

  lines.forEach((line, index) => {
    const change = prefixes[index];

    if (change) splices.push({ at: lineAt, remove: change.old, insert: change.prefix });
    lineAt += line.length + 1;
  });

  // From the last line up, so each splice's place is still where the text had it.
  splices.reverse();
  const delta = (change: { old: number; prefix: string }) => change.prefix.length - change.old;
  const firstDelta = prefixes[0] ? delta(prefixes[0]) : 0;
  const totalDelta = prefixes.reduce((sum, change) => sum + delta(change), 0);

  const selection = {
    start: Math.max(from, start + firstDelta),
    end: Math.max(from, end + totalDelta),
  };

  return edited(text, splices, ordered(selection));
}

/** A line without its list or quote prefix, and the indentation a list item had. */
function stripBlock(line: string) {
  const bullet = BULLET.exec(line);

  if (bullet) return { indent: bullet[1] ?? '', text: line.slice(bullet[0].length) };
  const numbered = NUMBERED.exec(line);

  if (numbered) return { indent: numbered[1] ?? '', text: line.slice(numbered[0].length) };
  const quote = QUOTE.exec(line);

  if (quote) return { indent: '', text: line.slice(quote[0].length) };

  return { indent: '', text: line };
}

function toggleFence(text: string, { start, end }: Selection): Edited {
  const from = lineStart(text, start);
  const to = lineEnd(text, end);
  // Already fenced exactly: the fences go.
  const above = from > 0 ? text.slice(lineStart(text, from - 1), from - 1) : null;
  const below = to < text.length ? text.slice(to + 1, lineEnd(text, to + 1)) : null;

  if (above !== null && below !== null && FENCE.test(above) && FENCE.test(below)) {
    const aboveStart = lineStart(text, from - 1);
    const after = lineEnd(text, to + 1);
    const shift = from - aboveStart;

    return edited(text, [removal(to, after - to), removal(aboveStart, shift)], {
      start: start - shift,
      end: end - shift,
    });
  }

  const opening = from > 0 && text[from - 1] !== '\n' ? '\n```\n' : '```\n';

  return edited(text, [insertion(to, '\n```'), insertion(from, opening)], {
    start: start + opening.length,
    end: end + opening.length,
  });
}

// ----- sending ---------------------------------------------------------------------------------

/**
 * What a draft sends: the plain body, Matrix HTML when the text carries formatting or pills, and
 * whom its pills mention.
 */
export interface OutgoingText {
  body: string;
  html: string | null;
  mentions: Mentions;
}

interface Marks {
  bold: boolean;
  italic: boolean;
  strike: boolean;
  code: boolean;
  /** Hidden until tapped (`||…||`): `[Spoiler]` in the plain body. */
  spoiler: boolean;
  /** A formula (`$$…$$`): the run's text is its LaTeX. */
  math: boolean;
  link: string | null;
}

interface Run {
  text: string;
  marks: Marks;
}

type BlockKind =
  | { type: 'paragraph' }
  | { type: 'quote' }
  | { type: 'code' }
  | { type: 'bullet' | 'numbered'; level: number };

interface Block {
  kind: BlockKind;
  runs: Run[];
  /** The line starts inside a spoiler the line before opened (`bridgeSpoilers`). */
  continuesSpoiler?: boolean;
}

const PLAIN: Marks = {
  bold: false,
  italic: false,
  strike: false,
  code: false,
  spoiler: false,
  math: false,
  link: null,
};

/**
 * Where pills stand while a draft is parsed: each is one character of Unicode's private use area,
 * so markers around a pill format it and nothing inside its label reads as a marker.
 */
const PILL_BASE = 0xe000;

const PILL_CHAR = /[\ue000-\uf8ff]/g;

/** The body and HTML the desktop sends for the same text, formatting and pills. */
export function outgoingText(draft: string, given: readonly DraftPill[] = []): OutgoingText {
  const text = draft.replace(/\r\n?/g, '\n');

  // Line ends written as one character move the pills after them.
  const pills =
    text === draft
      ? given
      : given.map((pill) => ({
          ...pill,
          label: pill.label.replace(/\r\n?/g, '\n'),
          start: draft.slice(0, pill.start).replace(/\r\n?/g, '\n').length,
        }));

  const placed = placePills(text, pills).slice(0, 0x1900);
  let source = '';
  let at = 0;

  placed.forEach((p, i) => {
    source += text.slice(at, p.start) + String.fromCharCode(PILL_BASE + i);
    at = p.end;
  });
  source += text.slice(at);
  const found = placed.map((p) => p.pill);
  const blocks = parseBlocks(source);

  // A spoiler over several lines is one `[Spoiler]`, its line breaks hidden with its words.
  const plain = blocks
    .map((block, i) => {
      const continued = block.continuesSpoiler === true;

      return (i === 0 || continued ? '' : '\n') + plainRuns(block.runs, continued);
    })
    .join('');

  const body = expandPills(plain.trim(), found, pillText);

  const formatted =
    found.some(pillFormats) ||
    blocks.some(
      (block) => block.kind.type !== 'paragraph' || block.runs.some((run) => !isPlain(run.marks)),
    );

  const mentions = mentionsOf(text, pills);

  if (!formatted || body === '') return { body, html: null, mentions };

  return { body, html: serialize(trimBlocks(blocks), found), mentions };
}

/**
 * A line's plain text: a spoiler's words hidden behind `[Spoiler]`, a formula as its LaTeX.
 * `continued` when the line starts inside a spoiler the line before already wrote.
 */
function plainRuns(runs: Run[], continued: boolean): string {
  let out = '';
  let hidden = continued;

  for (const run of runs) {
    if (run.marks.spoiler) {
      if (!hidden) out += SPOILER_FALLBACK;
      hidden = true;
      continue;
    }

    hidden = false;
    out += run.text;
  }

  return out;
}

/** `text` with each pill's character written as `write` says. */
function expandPills(text: string, pills: readonly Pill[], write: (pill: Pill) => string): string {
  return text.replace(PILL_CHAR, (c) => {
    const pill = pills[c.charCodeAt(0) - PILL_BASE];

    return pill ? write(pill) : c;
  });
}

/** Whether a draft has anything to send as text. */
export function hasText(draft: string): boolean {
  return outgoingText(draft).body !== '';
}

function isPlain(marks: Marks): boolean {
  return (
    !marks.bold &&
    !marks.italic &&
    !marks.strike &&
    !marks.code &&
    !marks.spoiler &&
    !marks.math &&
    marks.link === null
  );
}

/** Empty paragraphs before and after the text are dropped, as the body is trimmed. */
function trimBlocks(blocks: Block[]): Block[] {
  const empty = (block: Block) =>
    block.kind.type === 'paragraph' && block.runs.every((run) => run.text.trim() === '');

  let from = 0;
  let to = blocks.length;

  while (from < to && empty(blocks[from]!)) from++;

  while (to > from && empty(blocks[to - 1]!)) to--;

  return blocks.slice(from, to);
}

/** A line of the draft: its block, and its text after the block's prefix (`- `, `> `). */
interface Line {
  kind: BlockKind;
  text: string;
  /** Code, read as written; `null` for a fence, which ends what spoilers can span. */
  code: boolean | null;
}

function parseBlocks(text: string): Block[] {
  const lines: Line[] = [];
  let fenced = false;
  let previous: BlockKind | undefined;

  for (const line of text.split('\n')) {
    if (FENCE.test(line)) {
      // A whole code span on one line: ```code```.
      const single = /^ *```(.+)```\s*$/.exec(line);

      if (!fenced && single) {
        previous = { type: 'code' };
        lines.push({ kind: previous, text: single[1] ?? '', code: true });
        continue;
      }

      fenced = !fenced;
      lines.push({ kind: { type: 'code' }, text: '', code: null });
      continue;
    }

    if (fenced) {
      previous = { type: 'code' };
      lines.push({ kind: previous, text: line, code: true });
      continue;
    }

    const previousLevel =
      previous && (previous.type === 'bullet' || previous.type === 'numbered')
        ? previous.level
        : -1;

    const bullet = BULLET.exec(line);
    const numbered = bullet ? null : NUMBERED.exec(line);
    const quote = bullet || numbered ? null : QUOTE.exec(line);
    const item = bullet ?? numbered;

    if (item) {
      // Two spaces a level, at most one level deeper than the item above.
      const level = Math.min(Math.floor((item[1] ?? '').length / 2), previousLevel + 1);
      previous = { type: bullet ? 'bullet' : 'numbered', level };
      lines.push({ kind: previous, text: line.slice(item[0].length), code: false });
    } else if (quote) {
      previous = { type: 'quote' };
      lines.push({ kind: previous, text: line.slice(quote[0].length), code: false });
    } else {
      previous = { type: 'paragraph' };
      lines.push({ kind: previous, text: line, code: false });
    }
  }

  const continued = bridgeSpoilers(lines);

  return lines.flatMap((line, i): Block[] => {
    if (line.code === null) return [];

    if (line.code) return [{ kind: line.kind, runs: [{ text: line.text, marks: PLAIN }] }];

    return [{ kind: line.kind, runs: parseInline(line.text), continuesSpoiler: continued[i] }];
  });
}

/**
 * Spoilers that span lines (`||he` … `dies||`, as `editDraft` writes a spoiler with line breaks
 * in it) closed at the end of each line they cover and opened again at the start of the next,
 * after its prefix, so each line reads its part hidden. A spoiler spans only lines of text, not
 * a code block, and only when a later line closes it: a lone `||` stays text. Which lines start
 * inside such a spoiler.
 */
function bridgeSpoilers(lines: Line[]): boolean[] {
  let open = false;

  return lines.map((line, i) => {
    if (line.code !== false) {
      open = false;

      return false;
    }

    const continued = open;

    if (open) {
      // A blank line inside a spoiler stays in it.
      if (line.text.trim() === '') return true;
      const lead = line.text.length - line.text.trimStart().length;
      line.text = `${line.text.slice(0, lead)}||${line.text.slice(lead)}`;
    }

    open = unclosedSpoiler(line.text) >= 0 && closedLater(lines, i + 1);

    if (open) {
      const words = line.text.trimEnd();
      line.text = `${words}||${line.text.slice(words.length)}`;
    }

    return continued;
  });
}

/** Whether a line from `from` on, before a code block, closes a spoiler opened above it. */
function closedLater(lines: readonly Line[], from: number): boolean {
  for (const line of lines.slice(from)) {
    if (line.code !== false) return false;
    const text = `||${line.text.trimStart()}`;

    if (line.text.trim() !== '' && findClose(text, 2, '||') >= 0) return true;
  }

  return false;
}

/**
 * Where one line opens a spoiler it doesn't close, as `inline` reads it (code, formulas and
 * links skipped, other formatting's pairs passed over); -1 when it doesn't.
 */
function unclosedSpoiler(source: string): number {
  let i = 0;

  outer: while (i < source.length) {
    const c = source[i]!;

    if (c === '\\' && i + 1 < source.length && ESCAPABLE.includes(source[i + 1]!)) {
      i += 2;
      continue;
    }

    if (c === '`') {
      const close = source.indexOf('`', i + 1);

      if (close > i + 1) {
        i = close + 1;
        continue;
      }
    }

    if (source.startsWith('$$', i)) {
      const close = source.indexOf('$$', i + 2);

      if (close > i + 2) {
        i = close + 2;
        continue;
      }
    }

    if (c === '[') {
      const link = LINK.exec(source.slice(i));

      if (link) {
        i += link[0].length;
        continue;
      }
    }

    for (const [delimiter, mark] of DELIMITERS) {
      if (!source.startsWith(delimiter, i) || !canOpen(source, i, delimiter)) continue;
      const close = findClose(source, i + delimiter.length, delimiter);

      if (close < 0) {
        if (mark === 'spoiler') return i;
        continue;
      }

      i = close + delimiter.length;
      continue outer;
    }

    i++;
  }

  return -1;
}

const ESCAPABLE = '\\`*_~[]()>#-.!|$';

/** A link written in the draft: `[label](address)`. */
const LINK = /^\[([^\]]+)\]\(([^)\s]+)\)/;

/** The runs of one line: its text cut where the marks around it change. */
export function parseInline(source: string): Run[] {
  const runs: Run[] = [];
  inline(source, PLAIN, runs);
  // Adjacent runs with the same marks are one, as Rune keeps marks canonical.
  const merged: Run[] = [];

  for (const run of runs) {
    const last = merged[merged.length - 1];

    if (run.text === '') continue;

    if (last && sameMarks(last.marks, run.marks)) last.text += run.text;
    else merged.push({ ...run });
  }

  return merged;
}

function sameMarks(a: Marks, b: Marks): boolean {
  return (
    a.bold === b.bold &&
    a.italic === b.italic &&
    a.strike === b.strike &&
    a.code === b.code &&
    a.spoiler === b.spoiler &&
    a.math === b.math &&
    a.link === b.link
  );
}

const DELIMITERS: ReadonlyArray<[string, 'bold' | 'italic' | 'strike' | 'spoiler']> = [
  ['**', 'bold'],
  ['~~', 'strike'],
  ['||', 'spoiler'],
  ['_', 'italic'],
  ['*', 'italic'],
];

function inline(source: string, marks: Marks, out: Run[]): void {
  let plain = '';

  const flush = () => {
    if (plain) out.push({ text: plain, marks });
    plain = '';
  };

  let i = 0;

  outer: while (i < source.length) {
    const c = source[i]!;

    if (c === '\\' && i + 1 < source.length && ESCAPABLE.includes(source[i + 1]!)) {
      plain += source[i + 1];
      i += 2;
      continue;
    }

    if (c === '`') {
      const close = source.indexOf('`', i + 1);

      if (close > i + 1) {
        flush();
        out.push({ text: source.slice(i + 1, close), marks: { ...marks, code: true } });
        i = close + 1;
        continue;
      }
    }

    if (source.startsWith('$$', i)) {
      const close = source.indexOf('$$', i + 2);

      if (close > i + 2) {
        flush();
        out.push({ text: source.slice(i + 2, close), marks: { ...marks, math: true } });
        i = close + 2;
        continue;
      }
    }

    if (c === '[') {
      const link = LINK.exec(source.slice(i));

      if (link) {
        flush();
        inline(link[1] ?? '', { ...marks, link: link[2] ?? '' }, out);
        i += link[0].length;
        continue;
      }
    }

    for (const [delimiter, mark] of DELIMITERS) {
      if (!source.startsWith(delimiter, i) || !canOpen(source, i, delimiter)) continue;
      const close = findClose(source, i + delimiter.length, delimiter);

      if (close < 0) continue;
      flush();
      inline(source.slice(i + delimiter.length, close), { ...marks, [mark]: true }, out);
      i = close + delimiter.length;
      continue outer;
    }

    plain += c;
    i++;
  }

  flush();
}

const WORD = /[\p{L}\p{N}]/u;

function canOpen(source: string, at: number, delimiter: string): boolean {
  const next = source[at + delimiter.length];

  if (next === undefined || /\s/.test(next)) return false;

  // `snake_case` isn't italic: an underscore opens only at the start of a word.
  if (delimiter === '_' && at > 0 && WORD.test(source[at - 1]!)) return false;

  // `**` isn't two `*`.
  if (delimiter === '*' && next === '*') return false;

  return true;
}

function findClose(source: string, from: number, delimiter: string): number {
  for (let j = from + 1; j <= source.length - delimiter.length; j++) {
    if (source[j - 1] === '\\') continue;

    if (!source.startsWith(delimiter, j)) continue;

    if (delimiter.length === 1 && source[j + 1] === delimiter) {
      // `**` inside `*…*` belongs to bold: skip both.
      j++;
      continue;
    }

    if (/\s/.test(source[j - 1]!)) continue;
    const after = source[j + delimiter.length];

    if (delimiter === '_' && after !== undefined && WORD.test(after)) continue;

    return j;
  }

  return -1;
}

// ----- Matrix HTML, as Rune's `serialize_html` writes it --------------------------------------

function escapeHtml(text: string): string {
  return text.replace(/[&<>"]/g, (c) =>
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;',
  );
}

function serializeRuns(runs: Run[], pills: readonly Pill[]): string {
  let out = '';
  let spoiler = false;

  for (const run of runs) {
    for (const part of spoilerParts(run.text, pills)) {
      // A spoiler pill is never code's or a formula's text, which would show it: those end
      // around it, and it goes as its own spoiler in the other marks.
      const marks = part.spoiler ? { ...run.marks, code: false, math: false } : run.marks;

      if (marks.spoiler !== spoiler) {
        out += marks.spoiler ? '<span data-mx-spoiler>' : '</span>';
        spoiler = marks.spoiler;
      }

      out += serializeRun(part.text, marks, pills);
    }
  }

  if (spoiler) out += '</span>';

  return out;
}

/** `text` cut so that each spoiler pill in it is a part of its own. */
function spoilerParts(text: string, pills: readonly Pill[]) {
  const parts: Array<{ text: string; spoiler: boolean }> = [];
  let plain = '';

  for (const c of text) {
    const pill = pillOf(c, pills);

    if (pill && isSpoiler(pill)) {
      if (plain) parts.push({ text: plain, spoiler: false });
      parts.push({ text: c, spoiler: true });
      plain = '';
    } else {
      plain += c;
    }
  }

  if (plain) parts.push({ text: plain, spoiler: false });

  return parts;
}

/** The pill a private-use character stands for while a draft is parsed. */
function pillOf(c: string, pills: readonly Pill[]): Pill | undefined {
  const code = c.charCodeAt(0);

  return c.length === 1 && code >= PILL_BASE && code <= 0xf8ff
    ? pills[code - PILL_BASE]
    : undefined;
}

function serializeRun(text: string, marks: Marks, pills: readonly Pill[]): string {
  if (marks.math) {
    // A formula's LaTeX is its fallback, as the engine's `from_pills` writes it.
    const latex = escapeHtml(expandPills(text, pills, (pill) => pill.label));

    return `<span data-mx-maths="${latex}"><code>${latex}</code></span>`;
  }

  // Rune's canonical nesting: link, bold, italic, strike, code.
  const open: string[] = [];
  const close: string[] = [];

  if (marks.link !== null) {
    open.push(`<a href="${escapeHtml(marks.link)}">`);
    close.unshift('</a>');
  }

  for (const [mark, tag] of [
    ['bold', 'strong'],
    ['italic', 'em'],
    ['strike', 's'],
    ['code', 'code'],
  ] as const) {
    if (marks[mark]) {
      open.push(`<${tag}>`);
      close.unshift(`</${tag}>`);
    }
  }

  // Inside code a pill is its label; elsewhere its HTML, a spoiler pill inside a spoiler its
  // words alone.
  const inner = escapeHtml(text);

  const written = marks.code
    ? expandPills(inner, pills, (pill) => escapeHtml(pill.label))
    : expandPills(inner, pills, (pill) =>
        marks.spoiler && isSpoiler(pill) ? spoilerHtml(pill.label) : pillHtml(pill),
      );

  return open.join('') + written + close.join('');
}

type Direction = 'ltr' | 'rtl';

const STRONG_RTL = /[֐-ࣿיִ-﷿ﹰ-﻿]/u;

const STRONG = /[\p{L}]/u;

/** The direction of a paragraph's first strong character, if any. */
function directionOf(text: string): Direction | null {
  for (const c of text) {
    if (STRONG_RTL.test(c)) return 'rtl';

    if (STRONG.test(c)) return 'ltr';
  }

  return null;
}

/** Each block's ` dir` attribute: only when some block reads right to left, as Rune does. */
function directions(blocks: Block[]): string[] {
  let last: Direction | null = null;

  const shown = blocks.map((block) => {
    last = directionOf(block.runs.map((run) => run.text).join('')) ?? last;

    return last;
  });

  const rtl = blocks.some((block, i) => block.kind.type !== 'code' && shown[i] === 'rtl');

  return blocks.map((block, i) => {
    if (!rtl) return '';

    if (block.kind.type === 'code' || shown[i] === 'ltr') return ' dir="ltr"';

    return shown[i] === 'rtl' ? ' dir="rtl"' : '';
  });
}

function serialize(blocks: Block[], pills: readonly Pill[]): string {
  const dirs = directions(blocks);
  let out = '';
  let i = 0;

  while (i < blocks.length) {
    const block = blocks[i]!;
    const kind = block.kind;

    switch (kind.type) {
      case 'code': {
        const lines: string[] = [];
        const dir = dirs[i];

        while (i < blocks.length && blocks[i]!.kind.type === 'code') {
          const line = escapeHtml(blocks[i]!.runs.map((run) => run.text).join(''));
          // A spoiler pill stays a spoiler: readers draw a code block holding one as text.
          lines.push(
            expandPills(line, pills, (pill) =>
              isSpoiler(pill) ? pillHtml(pill) : escapeHtml(pill.label),
            ),
          );
          i++;
        }

        out += `<pre${dir}><code>${lines.join('\n')}</code></pre>`;
        break;
      }

      case 'bullet':
      case 'numbered': {
        const open: Array<'ul' | 'ol'> = [];

        while (i < blocks.length) {
          const item = blocks[i]!.kind;

          if (item.type !== 'bullet' && item.type !== 'numbered') break;
          const list = item.type === 'bullet' ? 'ul' : 'ol';
          const depth = item.level + 1;

          while (open.length > depth) out += `</${open.pop()}>`;

          if (open.length === depth && open[open.length - 1] !== list) out += `</${open.pop()}>`;

          while (open.length < depth) {
            const next: 'ul' | 'ol' =
              open.length + 1 === depth ? list : (open[open.length - 1] ?? list);

            out += `<${next}${dirs[i]}>`;
            open.push(next);
          }

          out += `<li${dirs[i]}>${serializeRuns(blocks[i]!.runs, pills)}</li>`;
          i++;
        }

        while (open.length > 0) out += `</${open.pop()}>`;
        break;
      }

      case 'quote':
        out += `<blockquote${dirs[i]}>${serializeRuns(block.runs, pills)}</blockquote>`;
        i++;
        break;
      case 'paragraph':
        out += `<p${dirs[i]}>${serializeRuns(block.runs, pills)}</p>`;
        i++;
        break;
    }
  }

  return out;
}

// ----- editing: a message's formatting back into markers --------------------------------------

const ENTITIES = new Map(
  Object.entries({
    amp: '&',
    lt: '<',
    gt: '>',
    quot: '"',
    apos: "'",
    nbsp: ' ',
  }),
);

function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name: string) => {
    if (name[0] === '#') {
      const code =
        name[1] === 'x' || name[1] === 'X'
          ? parseInt(name.slice(2), 16)
          : parseInt(name.slice(1), 10);

      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }

    return ENTITIES.get(name.toLowerCase()) ?? whole;
  });
}

/**
 * The draft that shows a message for editing: its HTML written back as markers, or its body
 * when it has no HTML. Tags outside what the composer writes keep their text.
 */
export function draftOf(body: string, html: string | null): string {
  return editDraft(body, html).draft;
}

/** A message as the composer edits it: its draft, and the pills in it. */
export interface EditDraft {
  draft: string;
  pills: DraftPill[];
}

/** A message's draft for editing, with the pills its mentions and custom emoji come back as. */
export function editDraft(body: string, html: string | null): EditDraft {
  if (!html) return { draft: body, pills: [] };
  // Each pill is written as one private-use character while lines are joined and trimmed, then
  // as its label, so its place is known (as `outgoingText` marks them).
  const pills: Pill[] = [];

  const mark = (pill: Pill) => {
    pills.push(pill);

    return String.fromCharCode(PILL_BASE + ((pills.length - 1) % 0x1900));
  };

  /** A mention being read: what it stands for, and its words so far. */
  let mention: { kind: 'user' | 'room'; id: string; text: string } | null = null;
  const lines: string[] = [];
  let line = '';
  /** What starts each new line inside the current block (a quote's `> `, an item's indent). */
  let continuation = '';
  const lists: Array<{ ordered: boolean; count: number }> = [];
  let pre = false;
  let quote = 0;
  let skip = 0;
  /** Each open link's address; `null` for a link around a spoiler, which is no link. */
  const links: Array<string | null> = [];

  const breakLine = () => {
    lines.push(line);
    line = continuation;
  };

  /** Text as it reads, its line breaks breaking the line. */
  const writeOutside = (text: string) => {
    text.split('\n').forEach((part, index) => {
      if (index > 0) breakLine();
      line += part;
    });
  };

  const endBlock = () => {
    if (line !== continuation && line !== '') lines.push(line);
    line = '';
    continuation = quote > 0 ? '> ' : '';

    if (quote > 0) line = continuation;
  };

  const tokens = /<(\/?)([a-zA-Z][a-zA-Z0-9-]*)([^>]*)>|([^<]+)/g;

  for (let match = tokens.exec(html); match; match = tokens.exec(html)) {
    const [, closing, rawTag, attrs, text] = match;

    if (text !== undefined) {
      if (skip > 0) continue;

      if (mention) {
        mention.text += decodeEntities(text);
        continue;
      }

      if (pre) {
        const parts = decodeEntities(text).split('\n');
        parts.forEach((part, index) => {
          if (index > 0) breakLine();
          line += part;
        });
        continue;
      }

      // Whitespace between blocks (`</p>\n<p>`) is layout, not text.
      if (/^\s*$/.test(text) && text.includes('\n')) continue;
      line += decodeEntities(text.replace(/\s*\n\s*/g, ' '));
      continue;
    }

    const tag = (rawTag ?? '').toLowerCase();
    const end = closing === '/';

    if (tag === 'mx-reply') {
      skip += end ? -1 : 1;
      continue;
    }

    if (skip > 0) continue;

    if (mention) {
      if (tag === 'a' && end) {
        const words = mention.text.trim() || mention.id;

        const pill =
          mention.kind === 'user' ? userPill(mention.id, words) : roomPill(mention.id, words);

        line += mark(pill);
        mention = null;
      }

      continue;
    }

    if ((tag === 'span' || tag === 'div') && !end) {
      // A spoiler first: one also marked a formula would show its words as the formula.
      if (attribute(attrs ?? '', 'data-mx-spoiler') !== null) {
        // A spoiler is a pill of its words, as the engine's `to_pills` makes it: whatever they
        // say, nothing in them can end it or show it (`pills.ts`).
        const after = elementEnd(html, tag, tokens.lastIndex);
        const raw = spoilerWords(html.slice(tokens.lastIndex, after), pre);
        const words = raw.trim();

        if (tag === 'div') endBlock();
        // Space and breaks around its words stay outside, where they are seen.
        writeOutside(raw.slice(0, raw.length - raw.trimStart().length));

        if (words !== '') line += mark(spoilerPill(words));
        writeOutside(raw.slice(raw.trimEnd().length));
        tokens.lastIndex = after;

        if (tag === 'div') endBlock();
        continue;
      }

      const latex = attribute(attrs ?? '', 'data-mx-maths');

      if (latex !== null) {
        if (tag === 'div') endBlock();
        line += `$$${latex}$$`;
        // The fallback inside isn't the formula's text: skip to the element's end.
        tokens.lastIndex = elementEnd(html, tag, tokens.lastIndex);

        if (tag === 'div') endBlock();
        continue;
      }
    }

    switch (tag) {
      case 'strong':
      case 'b':
        line += '**';
        break;
      case 'em':
      case 'i':
        line += '_';
        break;
      case 's':
      case 'del':
      case 'strike':
        line += '~~';
        break;
      case 'code':
        if (!pre) line += '`';
        break;
      case 'img': {
        const src = attribute(attrs ?? '', 'src') ?? '';

        // A homeserver image is a custom emoji, marked or not (see `html.ts`).
        if (src.startsWith('mxc://')) {
          const code = attribute(attrs ?? '', 'alt') ?? attribute(attrs ?? '', 'title') ?? '';
          line += mark(emotePill(src, code.replace(/^:|:$/g, '') || 'emoji'));
        } else {
          line += attribute(attrs ?? '', 'alt') ?? '';
        }

        break;
      }

      case 'a':
        if (end) {
          const href = links.pop();
          line += href === undefined || href === null ? '' : `](${href})`;
        } else {
          const href = attribute(attrs ?? '', 'href') ?? '';
          const inner = html.slice(tokens.lastIndex, elementEnd(html, 'a', tokens.lastIndex));

          // A link around a spoiler is no link, nor a mention: its content stays as it is, the
          // spoiler a spoiler, as `html.ts` reads it.
          if (/data-mx-spoiler/i.test(inner)) {
            links.push(null);
            break;
          }

          const target = mentionOf(href);

          if (target) {
            mention = { ...target, text: '' };
            break;
          }

          links.push(href);
          line += '[';
        }

        break;
      case 'br':
        breakLine();
        break;
      case 'p':
      case 'div':
      case 'h1':
      case 'h2':
      case 'h3':
      case 'h4':
      case 'h5':
      case 'h6':
        endBlock();
        break;
      case 'pre':
        endBlock();

        if (end) {
          lines.push('```');
          pre = false;
        } else {
          lines.push('```');
          pre = true;
        }

        line = '';
        break;
      case 'blockquote':
        endBlock();
        quote = Math.max(0, quote + (end ? -1 : 1));
        continuation = quote > 0 ? '> ' : '';
        line = continuation;
        break;
      case 'ul':
      case 'ol':
        endBlock();

        if (end) lists.pop();
        else lists.push({ ordered: tag === 'ol', count: 0 });
        line = '';
        break;
      case 'li': {
        endBlock();

        if (end) {
          line = '';
          break;
        }

        const list = lists[lists.length - 1];
        const indent = '  '.repeat(Math.max(0, lists.length - 1));

        if (list) list.count++;
        const marker = list?.ordered ? `${list.count}. ` : '- ';
        line = indent + marker;
        continuation = indent + ' '.repeat(marker.length);
        break;
      }

      default:
        break;
    }
  }

  if (line !== '' && line !== continuation) lines.push(line);

  const draft = lines
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();

  return draft === '' ? { draft: body, pills: [] } : withPills(draft, pills);
}

/**
 * A spoiler's words from the HTML inside it (`pre` in a code block): its text, line breaks as
 * written (`<br>`), custom emoji and images as their descriptions, nothing else of its markup.
 */
function spoilerWords(inner: string, pre: boolean): string {
  let words = '';
  const tokens = /<(\/?)([a-zA-Z][a-zA-Z0-9-]*)([^>]*)>|([^<]+)/g;

  for (let match = tokens.exec(inner); match; match = tokens.exec(inner)) {
    const [, closing, rawTag, attrs, text] = match;

    if (text !== undefined) {
      // In a code block its text is as written; elsewhere line ends are the markup's layout.
      if (pre) words += decodeEntities(text);
      else if (!(/^\s*$/.test(text) && text.includes('\n')))
        words += decodeEntities(text.replace(/\s*\n\s*/g, ' '));
      continue;
    }

    const tag = (rawTag ?? '').toLowerCase();

    if (tag === 'br') words += '\n';
    else if (tag === 'img' && closing !== '/')
      words += attribute(attrs ?? '', 'alt') ?? attribute(attrs ?? '', 'title') ?? '';
  }

  return words;
}

/**
 * `marked` with its pills' characters (the `next`th pill's, in order) written as their labels,
 * and the pills where they stand. A private-use character the message itself held stays.
 */
function withPills(marked: string, pills: readonly Pill[]): EditDraft {
  const placed: DraftPill[] = [];
  let draft = '';

  for (const c of marked) {
    const pill = pills[placed.length];

    if (pill && c === String.fromCharCode(PILL_BASE + (placed.length % 0x1900))) {
      placed.push(pillAt(pill, draft.length));
      draft += pill.label;
    } else {
      draft += c;
    }
  }

  return { draft, pills: livePills(draft, placed) };
}

/** Where the `tag` element whose opening tag ends at `from` ends, nested ones included. */
function elementEnd(html: string, tag: string, from: number): number {
  const tags = new RegExp(`<${tag}\\b[^>]*>|</${tag}\\s*>`, 'gi');
  tags.lastIndex = from;
  let depth = 1;

  for (let next = tags.exec(html); next; next = tags.exec(html)) {
    depth += next[0].startsWith('</') ? -1 : 1;

    if (depth === 0) return tags.lastIndex;
  }

  return html.length;
}

/** An attribute's decoded value in a tag's attribute text; `''` for one without a value. */
function attribute(attrs: string, name: string): string | null {
  for (const match of attrs.matchAll(
    /([^\s"'>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g,
  )) {
    if (match[1]?.toLowerCase() === name)
      return decodeEntities(match[2] ?? match[3] ?? match[4] ?? '');
  }

  return null;
}
