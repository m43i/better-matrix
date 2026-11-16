// Formatting in the composer. The phone's native text field holds plain text, so formatting is
// written as markers in it (`**bold**`, `_italic_`, `~~strike~~`, `` `code` ``, `[text](url)`,
// `- ` and `1. ` lists, `> ` quotes, ``` fences), which the toolbar inserts and removes. On send,
// `outgoingText` turns the text into what the desktop's rich field sends: the plain body, and
// Matrix HTML serialised as Rune's `TextEdit::to_html` does (apps/desktop/src/state/composer.rs),
// only when the text carries formatting. `draftOf` goes the other way, for editing a message.

/** Every formatting action the toolbar offers, in its order (the desktop's `Format::ALL`). */
export const FORMATS = ['bold', 'italic', 'strike', 'code', 'link', 'bullets', 'numbers', 'quote', 'codeBlock'] as const;
export type Format = (typeof FORMATS)[number];

/** A selection in the draft, as the native text field reports it (UTF-16 offsets). */
export interface Selection {
  start: number;
  end: number;
}

/** The draft after a change, and where the caret or selection goes. */
export interface Edited {
  text: string;
  selection: Selection;
}

/** The markers of the inline formats. */
const INLINE: Partial<Record<Format, string>> = { bold: '**', italic: '_', strike: '~~', code: '`' };

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
      return { text, selection };
  }
}

/** `text` with the selection linked to `url`; with nothing selected, the address goes in as a link. */
export function applyLink(text: string, selection: Selection, url: string): Edited {
  const address = url.trim();
  const { start, end } = ordered(selection);
  if (!address || address === 'https://') return { text, selection };
  const label = text.slice(start, end) || address;
  const link = `[${label}](${address})`;
  const caret = start + link.length;
  return { text: text.slice(0, start) + link + text.slice(end), selection: { start: caret, end: caret } };
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
    const text = previous.slice(0, start) + previous.slice(start + prefix.length);
    return { text, selection: { start, end: start } };
  }
  const numbered = NUMBERED.exec(line);
  const following = numbered ? `${numbered[1]}${Number(numbered[2]) + 1}. ` : prefix;
  const caret = at + 1 + following.length;
  return { text: next.slice(0, at + 1) + following + next.slice(at + 1), selection: { start: caret, end: caret } };
}

function blockPrefix(line: string): string | null {
  const match = BULLET.exec(line) ?? NUMBERED.exec(line) ?? (QUOTE.test(line) ? /^> ?/.exec(line) : null);
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
      return { text: text.slice(0, at) + text.slice(start + m), selection: { start: at, end: at } };
    }
    const caret = start + m;
    return { text: text.slice(0, start) + marker + marker + text.slice(start), selection: { start: caret, end: caret } };
  }
  // Selecting a word often takes the space after it: the markers go around the word.
  let s = start;
  let e = end;
  while (s < e && /\s/.test(text[s] ?? '')) s++;
  while (e > s && /\s/.test(text[e - 1] ?? '')) e--;
  if (s === e) return { text, selection: { start, end } };
  if (text.slice(s - m, s) === marker && text.slice(e, e + m) === marker) {
    const unwrapped = text.slice(0, s - m) + text.slice(s, e) + text.slice(e + m);
    return { text: unwrapped, selection: { start: s - m, end: e - m } };
  }
  const inner = text.slice(s, e);
  if (inner.length > 2 * m && inner.startsWith(marker) && inner.endsWith(marker)) {
    const unwrapped = text.slice(0, s) + inner.slice(m, -m) + text.slice(e);
    return { text: unwrapped, selection: { start: s, end: e - 2 * m } };
  }
  const wrapped = text.slice(0, s) + marker + inner + marker + text.slice(e);
  return { text: wrapped, selection: { start: s + m, end: e + m } };
}

function toggleLines(text: string, { start, end }: Selection, format: 'bullets' | 'numbers' | 'quote'): Edited {
  const from = lineStart(text, start);
  const to = lineEnd(text, end);
  const lines = text.slice(from, to).split('\n');
  const has = (line: string) =>
    format === 'bullets' ? BULLET.test(line) : format === 'numbers' ? NUMBERED.test(line) : QUOTE.test(line);
  const off = lines.every(has);
  const changed = lines.map((line, index) => {
    const bare = stripBlock(line);
    if (off) return bare.text;
    switch (format) {
      case 'bullets':
        return `${bare.indent}- ${bare.text.trimStart()}`;
      case 'numbers':
        return `${bare.indent}${index + 1}. ${bare.text.trimStart()}`;
      case 'quote':
        return `> ${bare.text}`;
    }
  });
  const first = changed[0] ?? '';
  const firstDelta = first.length - (lines[0] ?? '').length;
  const replaced = changed.join('\n');
  const totalDelta = replaced.length - (to - from);
  const selection = { start: Math.max(from, start + firstDelta), end: Math.max(from, end + totalDelta) };
  return { text: text.slice(0, from) + replaced + text.slice(to), selection: ordered(selection) };
}

/** A line without its list or quote prefix, and the indentation a list item had. */
function stripBlock(line: string): { indent: string; text: string } {
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
    const inner = text.slice(from, to);
    const after = lineEnd(text, to + 1);
    const removed = text.slice(0, aboveStart) + inner + text.slice(after);
    const shift = from - aboveStart;
    return { text: removed, selection: { start: start - shift, end: end - shift } };
  }
  const opening = from > 0 && text[from - 1] !== '\n' ? '\n```\n' : '```\n';
  const fenced = text.slice(0, from) + opening + text.slice(from, to) + '\n```' + text.slice(to);
  return { text: fenced, selection: { start: start + opening.length, end: end + opening.length } };
}

// ----- sending ---------------------------------------------------------------------------------

/** What a draft sends: the plain body, and Matrix HTML when the text carries formatting. */
export interface OutgoingText {
  body: string;
  html: string | null;
}

interface Marks {
  bold: boolean;
  italic: boolean;
  strike: boolean;
  code: boolean;
  link: string | null;
}

interface Run {
  text: string;
  marks: Marks;
}

type BlockKind = { type: 'paragraph' } | { type: 'quote' } | { type: 'code' } | { type: 'bullet' | 'numbered'; level: number };

interface Block {
  kind: BlockKind;
  runs: Run[];
}

const PLAIN: Marks = { bold: false, italic: false, strike: false, code: false, link: null };

/** The body and HTML the desktop sends for the same text and formatting. */
export function outgoingText(draft: string): OutgoingText {
  const blocks = parseBlocks(draft.replace(/\r\n?/g, '\n'));
  const plain = blocks.map((block) => block.runs.map((run) => run.text).join(''));
  const body = plain.join('\n').trim();
  const formatted = blocks.some((block) => block.kind.type !== 'paragraph' || block.runs.some((run) => !isPlain(run.marks)));
  if (!formatted || body === '') return { body, html: null };
  return { body, html: serialize(trimBlocks(blocks)) };
}

/** Whether a draft has anything to send as text. */
export function hasText(draft: string): boolean {
  return outgoingText(draft).body !== '';
}

function isPlain(marks: Marks): boolean {
  return !marks.bold && !marks.italic && !marks.strike && !marks.code && marks.link === null;
}

/** Empty paragraphs before and after the text are dropped, as the body is trimmed. */
function trimBlocks(blocks: Block[]): Block[] {
  const empty = (block: Block) => block.kind.type === 'paragraph' && block.runs.every((run) => run.text.trim() === '');
  let from = 0;
  let to = blocks.length;
  while (from < to && empty(blocks[from]!)) from++;
  while (to > from && empty(blocks[to - 1]!)) to--;
  return blocks.slice(from, to);
}

function parseBlocks(text: string): Block[] {
  const blocks: Block[] = [];
  let fenced = false;
  for (const line of text.split('\n')) {
    if (FENCE.test(line)) {
      // A whole code span on one line: ```code```.
      const single = /^ *```(.+)```\s*$/.exec(line);
      if (!fenced && single) {
        blocks.push({ kind: { type: 'code' }, runs: [{ text: single[1] ?? '', marks: PLAIN }] });
        continue;
      }
      fenced = !fenced;
      continue;
    }
    if (fenced) {
      blocks.push({ kind: { type: 'code' }, runs: [{ text: line, marks: PLAIN }] });
      continue;
    }
    const previous = blocks[blocks.length - 1]?.kind;
    const previousLevel = previous && (previous.type === 'bullet' || previous.type === 'numbered') ? previous.level : -1;
    const bullet = BULLET.exec(line);
    const numbered = bullet ? null : NUMBERED.exec(line);
    const quote = bullet || numbered ? null : QUOTE.exec(line);
    if (bullet || numbered) {
      const match = (bullet ?? numbered)!;
      // Two spaces a level, at most one level deeper than the item above.
      const level = Math.min(Math.floor((match[1] ?? '').length / 2), previousLevel + 1);
      const type = bullet ? 'bullet' : 'numbered';
      blocks.push({ kind: { type, level }, runs: parseInline(line.slice(match[0].length)) });
    } else if (quote) {
      blocks.push({ kind: { type: 'quote' }, runs: parseInline(line.slice(quote[0].length)) });
    } else {
      blocks.push({ kind: { type: 'paragraph' }, runs: parseInline(line) });
    }
  }
  return blocks;
}

const ESCAPABLE = '\\`*_~[]()>#-.!';

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
  return a.bold === b.bold && a.italic === b.italic && a.strike === b.strike && a.code === b.code && a.link === b.link;
}

const DELIMITERS: ReadonlyArray<[string, keyof Omit<Marks, 'link' | 'code'>]> = [
  ['**', 'bold'],
  ['~~', 'strike'],
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
    if (c === '[') {
      const link = /^\[([^\]]+)\]\(([^)\s]+)\)/.exec(source.slice(i));
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
  return text.replace(/[&<>"]/g, (c) => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;'));
}

function serializeRuns(runs: Run[]): string {
  let out = '';
  for (const { text, marks } of runs) {
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
    out += open.join('') + escapeHtml(text) + close.join('');
  }
  return out;
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

function serialize(blocks: Block[]): string {
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
          lines.push(escapeHtml(blocks[i]!.runs.map((run) => run.text).join('')));
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
            const next: 'ul' | 'ol' = open.length + 1 === depth ? list : (open[open.length - 1] ?? list);
            out += `<${next}${dirs[i]}>`;
            open.push(next);
          }
          out += `<li${dirs[i]}>${serializeRuns(blocks[i]!.runs)}</li>`;
          i++;
        }
        while (open.length > 0) out += `</${open.pop()}>`;
        break;
      }
      case 'quote':
        out += `<blockquote${dirs[i]}>${serializeRuns(block.runs)}</blockquote>`;
        i++;
        break;
      case 'paragraph':
        out += `<p${dirs[i]}>${serializeRuns(block.runs)}</p>`;
        i++;
        break;
    }
  }
  return out;
}

// ----- editing: a message's formatting back into markers --------------------------------------

const ENTITIES: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' };

function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name: string) => {
    if (name[0] === '#') {
      const code = name[1] === 'x' || name[1] === 'X' ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }
    return ENTITIES[name.toLowerCase()] ?? whole;
  });
}

/**
 * The draft that shows a message for editing: its HTML written back as markers, or its body
 * when it has no HTML. Tags outside what the composer writes keep their text.
 */
export function draftOf(body: string, html: string | null): string {
  if (!html) return body;
  const lines: string[] = [];
  let line = '';
  /** What starts each new line inside the current block (a quote's `> `, an item's indent). */
  let continuation = '';
  const lists: Array<{ ordered: boolean; count: number }> = [];
  let pre = false;
  let quote = 0;
  let skip = 0;
  const links: string[] = [];
  const breakLine = () => {
    lines.push(line);
    line = continuation;
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
      case 'a':
        if (end) {
          const href = links.pop();
          line += href === undefined ? '' : `](${href})`;
        } else {
          const href = /href\s*=\s*"([^"]*)"|href\s*=\s*'([^']*)'/i.exec(attrs ?? '');
          links.push(decodeEntities(href?.[1] ?? href?.[2] ?? ''));
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
  const draft = lines.join('\n').replace(/\n{3,}/g, '\n\n').trim();
  return draft === '' ? body : draft;
}
