// Message formatting: Matrix HTML (`org.matrix.custom.html`) read into a small, safe tree the
// timeline draws with native text. HTML is never rendered as such: only the tags below become
// formatting, links keep web, mail and `matrix:` addresses only, and everything else is plain
// text. Bare web addresses become links too (but not inside code), as on the desktop. Matrix's
// inline objects become their own inlines, as the engine's `html.rs` reads them: mentions
// (pills), spoilers (`data-mx-spoiler`, hidden until tapped, their content its plain words only;
// a link or mention around one is neither, so nothing shows what it hides), maths
// (`data-mx-maths`, shown as Unicode by `math.ts`) and custom emoji (`<img src="mxc://…">`:
// Matrix HTML's only images are the homeserver's, and the SDK's sanitiser drops
// `data-mx-emoticon`, so the source marks one).
//
// The tokenizer is ours rather than htmlparser2's: htmlparser2 is ESM-only, which the app's Jest
// setup doesn't load. Matrix HTML is a small, well-formed subset; anything unexpected reads as
// text.

import { linkRanges } from './links';
import { mathToUnicode } from './math';
import { parsePermalink } from './permalinks';
import { SPOILER_FALLBACK } from './pills';

/** How an inline run of text is formatted. */
export interface Marks {
  bold?: true;
  italic?: true;
  strike?: true;
  underline?: true;
  code?: true;
  /** The address it links to: `http(s)`, `mailto`, `ftp` or `matrix:` only. */
  link?: string;
}

export type Inline =
  | { type: 'text'; text: string; marks: Marks }
  /** A person or a room, linked through matrix.to: drawn as a pill. */
  | { type: 'mention'; kind: 'user' | 'room'; id: string; label: string }
  /**
   * Hidden text (`data-mx-spoiler`), shown once tapped; `reason` is what the sender said of it.
   * Its content is its plain words only (a custom emoji its description), so no link, mention
   * or picture inside it shows anything before it is tapped.
   */
  | { type: 'spoiler'; text: string; reason: string }
  /** A formula: its LaTeX, and the Unicode line it reads as. A block formula has its own line. */
  | { type: 'math'; latex: string; text: string; block: boolean }
  /** A custom emoji: its `mxc://` image, and its short code (`:blobcat:`) to read. */
  | { type: 'emote'; url: string; label: string };

export type Block =
  | { type: 'paragraph'; inlines: Inline[] }
  | { type: 'heading'; level: number; inlines: Inline[] }
  | { type: 'code'; text: string }
  | { type: 'quote'; blocks: Block[] }
  | { type: 'list'; ordered: boolean; start: number; items: Block[][] }
  | { type: 'rule' };

/** A message's plain text as blocks: one paragraph, its web addresses made links. */
export function plainBlocks(body: string): Block[] {
  return [{ type: 'paragraph', inlines: linkify([{ type: 'text', text: body, marks: {} }]) }];
}

/** A message's Matrix HTML as blocks. */
export function htmlBlocks(html: string): Block[] {
  return toBlocks(buildTree(html).children, {});
}

/**
 * Whether a message's Matrix HTML holds a spoiler (an element marked `data-mx-spoiler`), shown
 * or not: such a message offers no link to preview or GIF to play, since the plain body a link
 * is taken from may name what the spoiler hides.
 */
export function hasSpoiler(html: string): boolean {
  return /data-mx-spoiler/i.test(html) && markedSpoiler(buildTree(html));
}

function markedSpoiler(node: Node): boolean {
  return (
    node.type === 'element' &&
    (node.attrs['data-mx-spoiler'] !== undefined || node.children.some(markedSpoiler))
  );
}

/**
 * What a plain projection does with a spoiler: its words (`'words'`, for the spoiler's own
 * placeholder once tapped), or `[Spoiler]` (`'hidden'`, for anything that leaves the message:
 * copying, translating), as the plain body a sender writes for one says.
 */
export type SpoilerText = 'words' | 'hidden';

/**
 * The text inlines show, as plain words: for copying, accessibility, and a spoiler's hidden
 * placeholder (which draws none of its links, mentions or pictures).
 */
export function inlinesText(inlines: Inline[], spoilers: SpoilerText = 'words'): string {
  return inlines
    .map((i) => {
      switch (i.type) {
        case 'text':
          return i.text;
        case 'spoiler':
          return spoilers === 'hidden' ? SPOILER_FALLBACK : i.text;
        case 'math':
          return i.text;
        default:
          return i.label;
      }
    })
    .join('');
}

/**
 * A message's words as plain text that may leave it (a list's preview, a reply's excerpt): its
 * plain body, unless its HTML holds a spoiler, whose words a sender may have left in the body
 * while hiding them in the HTML. Then the text the timeline shows, each spoiler `[Spoiler]`.
 */
export function spoilerSafeText(body: string, html: string | null): string {
  return html !== null && hasSpoiler(html) ? blocksText(htmlBlocks(html), 'hidden') : body;
}

/** The text a tree shows, for copying and accessibility; spoilers as `spoilers` says. */
export function blocksText(blocks: Block[], spoilers: SpoilerText = 'words'): string {
  return blocks
    .map((block): string => {
      switch (block.type) {
        case 'paragraph':
        case 'heading':
          return inlinesText(block.inlines, spoilers);
        case 'code':
          return block.text;
        case 'quote':
          return blocksText(block.blocks, spoilers);
        case 'list':
          return block.items.map((item) => blocksText(item, spoilers)).join('\n');
        case 'rule':
          return '';
      }
    })
    .join('\n');
}

// ----- tokenizing --------------------------------------------------------------------------

interface Element {
  type: 'element';
  name: string;
  attrs: Record<string, string>;
  children: Node[];
}

type Node = Element | { type: 'text'; text: string };

/** Elements with no content. */
const VOID = new Set(['br', 'hr', 'img', 'input', 'meta', 'link', 'wbr', 'area', 'col', 'source']);

/** Elements whose content is never shown. */
const DROPPED = new Set([
  'mx-reply',
  'script',
  'style',
  'head',
  'title',
  'iframe',
  'object',
  'template',
]);

/** Elements that close an open paragraph, as HTML has it. */
const CLOSES_P = new Set([
  'p',
  'div',
  'ul',
  'ol',
  'blockquote',
  'pre',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'hr',
  'table',
]);

/** Deeper nesting reads as text: keeps the converter's recursion bounded. */
const MAX_DEPTH = 64;

const ENTITIES = new Map(
  Object.entries({
    amp: '&',
    lt: '<',
    gt: '>',
    quot: '"',
    apos: "'",
    nbsp: ' ',
  }),
);

function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (whole, name: string) => {
    if (name[0] === '#') {
      const code =
        name[1] === 'x' || name[1] === 'X'
          ? parseInt(name.slice(2), 16)
          : parseInt(name.slice(1), 10);

      return Number.isFinite(code) && code > 0 && code <= 0x10ffff
        ? String.fromCodePoint(code)
        : whole;
    }

    return ENTITIES.get(name.toLowerCase()) ?? whole;
  });
}

const OPEN_TAG =
  /<([a-zA-Z][a-zA-Z0-9-]*)((?:\s+[^\s"'>/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*)\s*(\/?)>/y;

const CLOSE_TAG = /<\/([a-zA-Z][a-zA-Z0-9-]*)\s*>/y;

const ATTRIBUTE = /([^\s"'>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;

function parseAttributes(source: string) {
  const attrs: Record<string, string> = {};

  for (const match of source.matchAll(ATTRIBUTE)) {
    const name = match[1]?.toLowerCase();

    if (name) attrs[name] = decodeEntities(match[2] ?? match[3] ?? match[4] ?? '');
  }

  return attrs;
}

/** The HTML as a tree of elements and text. Forgiving: stray closing tags are ignored. */
function buildTree(html: string): Element {
  const root: Element = { type: 'element', name: '#root', attrs: {}, children: [] };
  const stack: Element[] = [root];
  const top = () => stack[stack.length - 1] ?? root;

  const addText = (text: string) => {
    if (text === '') return;
    const children = top().children;
    const last = children[children.length - 1];

    if (last?.type === 'text') last.text += text;
    else children.push({ type: 'text', text });
  };

  const closeTo = (name: string): boolean => {
    for (let i = stack.length - 1; i > 0; i--) {
      if (stack[i]?.name === name) {
        stack.length = i;

        return true;
      }
    }

    return false;
  };

  let i = 0;

  while (i < html.length) {
    const lt = html.indexOf('<', i);

    if (lt < 0) {
      addText(decodeEntities(html.slice(i)));
      break;
    }

    addText(decodeEntities(html.slice(i, lt)));
    i = lt;

    if (html.startsWith('<!--', i)) {
      const end = html.indexOf('-->', i + 4);
      i = end < 0 ? html.length : end + 3;
      continue;
    }

    CLOSE_TAG.lastIndex = i;
    const close = CLOSE_TAG.exec(html);

    if (close) {
      closeTo((close[1] ?? '').toLowerCase());
      i = CLOSE_TAG.lastIndex;
      continue;
    }

    OPEN_TAG.lastIndex = i;
    const open = OPEN_TAG.exec(html);

    if (open) {
      i = OPEN_TAG.lastIndex;
      const name = (open[1] ?? '').toLowerCase();

      if (name === 'script' || name === 'style') {
        // Raw text up to its end tag, never shown.
        const end = html.toLowerCase().indexOf(`</${name}`, i);
        i = end < 0 ? html.length : html.indexOf('>', end) + 1 || html.length;
        continue;
      }

      if (CLOSES_P.has(name) && top().name === 'p') stack.pop();

      if (name === 'li' && top().name === 'li') stack.pop();

      if (stack.length > MAX_DEPTH) continue;

      const element: Element = {
        type: 'element',
        name,
        attrs: parseAttributes(open[2] ?? ''),
        children: [],
      };

      top().children.push(element);

      if (!VOID.has(name) && open[3] !== '/') stack.push(element);
      continue;
    }

    if (html.startsWith('<!', i) || html.startsWith('<?', i)) {
      const end = html.indexOf('>', i);
      i = end < 0 ? html.length : end + 1;
      continue;
    }

    addText('<');
    i += 1;
  }

  return root;
}

// ----- converting --------------------------------------------------------------------------

const SAFE_LINK = /^(https?:|mailto:|ftp:|matrix:)/i;

/**
 * A permalink's person or room, for a mention pill (matrix.to or `matrix:`, read as the engine
 * reads them). A link to a message stays a link, so it opens at the message.
 */
export function mentionOf(href: string): { kind: 'user' | 'room'; id: string } | null {
  const link = parsePermalink(href);

  if (!link || link.event !== null) return null;

  return { kind: link.kind, id: link.id };
}

/** Whether a spoiler lies within `node`. */
function holdsSpoiler(node: Node): boolean {
  return (
    node.type === 'element' &&
    !DROPPED.has(node.name) &&
    (((node.name === 'span' || node.name === 'div') &&
      node.attrs['data-mx-spoiler'] !== undefined) ||
      node.children.some(holdsSpoiler))
  );
}

/**
 * A spoiler's content as its plain words, as the engine reads it: text, line breaks, and the
 * descriptions of pictures (a custom emoji's short code); nothing a link, mention or picture
 * would show.
 */
function spoilerText(node: Node): string {
  if (node.type === 'text') return node.text.replace(/[ \t\n\r\f]+/g, ' ');

  if (DROPPED.has(node.name)) return '';

  if (node.name === 'br') return '\n';

  if (node.name === 'img') return node.attrs.alt ?? node.attrs.title ?? '';
  const inner = node.children.map(spoilerText).join('');

  return isBlock(node.name) ? `${inner}\n` : inner;
}

/**
 * A code block's content without its code: `<code>` gone (its content kept) and each line end
 * of its text a `<br>`, so it reads as text in the lines it was written in.
 */
function withoutCode(nodes: Node[]): Node[] {
  return nodes.flatMap((node): Node[] => {
    if (node.type === 'text')
      return node.text
        .split('\n')
        .flatMap((part, index): Node[] => [
          ...(index > 0 ? [{ type: 'element' as const, name: 'br', attrs: {}, children: [] }] : []),
          ...(part === '' ? [] : [{ type: 'text' as const, text: part }]),
        ]);

    if (DROPPED.has(node.name)) return [];

    if (node.name === 'code') return withoutCode(node.children);

    return [{ ...node, children: withoutCode(node.children) }];
  });
}

/** The text an element holds, as written (for code blocks). */
function rawText(node: Node): string {
  if (node.type === 'text') return node.text;

  if (DROPPED.has(node.name)) return '';

  if (node.name === 'br') return '\n';

  return node.children.map(rawText).join('');
}

const HEADINGS = new Map(Object.entries({ h1: 1, h2: 2, h3: 3, h4: 4, h5: 5, h6: 6 }));

const BLOCK_WRAPPERS = new Set([
  'p',
  'div',
  'details',
  'summary',
  'table',
  'thead',
  'tbody',
  'tfoot',
  'tr',
  'caption',
  'section',
  'article',
  'header',
  'footer',
  'figure',
  'dl',
  'dt',
  'dd',
  '#root',
]);

function isBlock(name: string): boolean {
  return (
    BLOCK_WRAPPERS.has(name) ||
    HEADINGS.has(name) ||
    name === 'blockquote' ||
    name === 'ul' ||
    name === 'ol' ||
    name === 'li' ||
    name === 'pre' ||
    name === 'hr'
  );
}

/** The blocks `nodes` make: runs of inline content become paragraphs. */
function toBlocks(nodes: Node[], marks: Marks): Block[] {
  const blocks: Block[] = [];
  let run: Node[] = [];

  const flush = () => {
    const inlines = tidy(linkify(run.flatMap((node) => toInlines(node, marks))));

    if (inlines.length > 0) blocks.push({ type: 'paragraph', inlines });
    run = [];
  };

  for (const node of nodes) {
    if (node.type === 'text' || !isBlock(node.name)) {
      run.push(node);
      continue;
    }

    flush();
    const level = HEADINGS.get(node.name);

    // A spoiler before a formula: one marked both stays hidden.
    if (node.name === 'div' && node.attrs['data-mx-spoiler'] !== undefined) {
      const inlines = toInlines({ ...node, name: 'span' }, marks);
      blocks.push({ type: 'paragraph', inlines });
    } else if (node.name === 'div' && node.attrs['data-mx-maths'] !== undefined) {
      blocks.push({ type: 'paragraph', inlines: [mathInline(node.attrs['data-mx-maths'], true)] });
    } else if (node.name === 'pre' && node.children.some(holdsSpoiler)) {
      // A code block would show a spoiler's words as code: one holding a spoiler is its text,
      // line for line, the spoiler hidden in it, as the engine reads it.
      blocks.push(...toBlocks(withoutCode(node.children), marks));
    } else if (level !== undefined) {
      const inlines = tidy(linkify(node.children.flatMap((child) => toInlines(child, marks))));

      if (inlines.length > 0) blocks.push({ type: 'heading', level, inlines });
    } else if (node.name === 'blockquote') {
      const inner = toBlocks(node.children, marks);

      if (inner.length > 0) blocks.push({ type: 'quote', blocks: inner });
    } else if (node.name === 'ul' || node.name === 'ol') {
      const items = node.children.flatMap((child) =>
        child.type === 'element' && child.name === 'li' ? [toBlocks(child.children, marks)] : [],
      );

      const start = parseInt(node.attrs.start ?? '1', 10);

      if (items.length > 0) {
        blocks.push({
          type: 'list',
          ordered: node.name === 'ol',
          start: Number.isFinite(start) ? start : 1,
          items,
        });
      }
    } else if (node.name === 'pre') {
      const text = rawText(node).replace(/\n$/, '');
      blocks.push({ type: 'code', text });
    } else if (node.name === 'hr') {
      blocks.push({ type: 'rule' });
    } else if (node.name === 'tr') {
      // A table row reads as one line, its cells apart.
      const cells = node.children.flatMap((cell) =>
        cell.type === 'element'
          ? [tidy(linkify(cell.children.flatMap((child) => toInlines(child, marks))))]
          : [],
      );

      const inlines = cells.flatMap((cell, index) =>
        index === 0 ? cell : [{ type: 'text' as const, text: ' · ', marks: {} }, ...cell],
      );

      if (inlines.length > 0) blocks.push({ type: 'paragraph', inlines });
    } else {
      blocks.push(...toBlocks(node.children, marks));
    }
  }

  flush();

  return blocks;
}

function withMark(marks: Marks, name: string): Marks {
  switch (name) {
    case 'b':
    case 'strong':
      return { ...marks, bold: true };
    case 'i':
    case 'em':
      return { ...marks, italic: true };
    case 'u':
      return { ...marks, underline: true };
    case 's':
    case 'strike':
    case 'del':
      return { ...marks, strike: true };
    case 'code':
      return { ...marks, code: true };
    default:
      return marks;
  }
}

/** One node's inline content. Blocks nested inside inline elements read as their text. */
function toInlines(node: Node, marks: Marks): Inline[] {
  if (node.type === 'text') {
    // Spaces collapse as in HTML; code keeps its own.
    return [
      {
        type: 'text',
        text: marks.code ? node.text : node.text.replace(/[ \t\n\r\f]+/g, ' '),
        marks,
      },
    ];
  }

  if (DROPPED.has(node.name)) return [];

  switch (node.name) {
    case 'br':
      return [{ type: 'text', text: '\n', marks }];
    case 'img': {
      const alt = node.attrs.alt;
      const src = node.attrs.src ?? '';

      // An image from the homeserver is a custom emoji, marked or not (the SDK's sanitiser
      // drops `data-mx-emoticon`), as the engine's `to_pills` reads it.
      if (src.startsWith('mxc://'))
        return [{ type: 'emote', url: src, label: alt ?? node.attrs.title ?? ':emoji:' }];

      // Other images can't show inline: their description does.
      return alt ? [{ type: 'text', text: alt, marks }] : [];
    }

    case 'span':
    case 'div': {
      // A spoiler before a formula: one marked both stays hidden.
      const reason = node.attrs['data-mx-spoiler'];

      if (reason !== undefined) {
        const text = node.children
          .map(spoilerText)
          .join('')
          .replace(/ *\n */g, '\n')
          .replace(/ {2,}/g, ' ')
          .trim();

        return text === '' ? [] : [{ type: 'spoiler', text, reason }];
      }

      const latex = node.attrs['data-mx-maths'];

      if (latex !== undefined) return [mathInline(latex, node.name === 'div')];

      const inlines = node.children.flatMap((child) => toInlines(child, marks));

      return node.name === 'div' ? [...inlines, { type: 'text', text: '\n', marks }] : inlines;
    }

    case 'a': {
      // A link around a spoiler is no link (nor a mention): its content shows as it is, the
      // spoiler still hidden, so neither the link's look nor its label gives the words away.
      if (node.children.some(holdsSpoiler))
        return node.children.flatMap((child) => toInlines(child, marks));
      const href = node.attrs.href ?? '';
      const label = node.children.map(rawText).join('').trim();
      const mention = mentionOf(href);

      if (mention)
        return [
          { type: 'mention', kind: mention.kind, id: mention.id, label: label || mention.id },
        ];
      const linked = SAFE_LINK.test(href) ? { ...marks, link: href } : marks;

      return node.children.flatMap((child) => toInlines(child, linked));
    }

    default: {
      const inner = withMark(marks, node.name);
      const inlines = node.children.flatMap((child) => toInlines(child, inner));

      // A block inside inline content (`<b><p>…</p></b>`) keeps its line.
      return isBlock(node.name) ? [...inlines, { type: 'text', text: '\n', marks }] : inlines;
    }
  }
}

function sameMarks(a: Marks, b: Marks): boolean {
  return (
    a.bold === b.bold &&
    a.italic === b.italic &&
    a.strike === b.strike &&
    a.underline === b.underline &&
    a.code === b.code &&
    a.link === b.link
  );
}

function mathInline(latex: string, block: boolean): Inline {
  return { type: 'math', latex, text: mathToUnicode(latex), block };
}

/** Inlines with runs of the same marks joined, spaces not doubled and the ends trimmed. */
function tidy(inlines: Inline[]): Inline[] {
  const out: Inline[] = [];

  for (const inline of inlines) {
    const last = out[out.length - 1];

    if (inline.type === 'text') {
      let text = inline.text;
      const before = last?.type === 'text' ? last.text : last ? 'x' : '\n';

      if (!inline.marks.code && /[ \n]$/.test(before)) text = text.replace(/^ +/, '');

      if (text === '') continue;

      if (last?.type === 'text' && sameMarks(last.marks, inline.marks)) {
        out[out.length - 1] = { ...last, text: last.text + text };
        continue;
      }

      out.push({ ...inline, text });
    } else {
      out.push(inline);
    }
  }

  // Trim the ends: spaces and line breaks from the markup's layout.
  while (out[0]?.type === 'text' && !out[0].marks.code && /^[ \n]*$/.test(out[0].text)) out.shift();
  const first = out[0];

  if (first?.type === 'text' && !first.marks.code)
    out[0] = { ...first, text: first.text.replace(/^[ \n]+/, '') };

  for (;;) {
    const last = out[out.length - 1];

    if (last?.type !== 'text' || last.marks.code) break;
    const text = last.text.replace(/[ \n]+$/, '');

    if (text === '') {
      out.pop();
      continue;
    }

    out[out.length - 1] = { ...last, text };
    break;
  }

  return out;
}

/** Inlines with every bare web address made a link, except in code and existing links. */
export function linkify(inlines: Inline[]): Inline[] {
  return inlines.flatMap((inline): Inline[] => {
    if (inline.type !== 'text' || inline.marks.code || inline.marks.link) return [inline];
    const ranges = linkRanges(inline.text);

    if (ranges.length === 0) return [inline];
    const parts: Inline[] = [];
    let at = 0;

    for (const [start, end] of ranges) {
      if (start > at)
        parts.push({ type: 'text', text: inline.text.slice(at, start), marks: inline.marks });
      const url = inline.text.slice(start, end);
      parts.push({ type: 'text', text: url, marks: { ...inline.marks, link: url } });
      at = end;
    }

    if (at < inline.text.length)
      parts.push({ type: 'text', text: inline.text.slice(at), marks: inline.marks });

    return parts;
  });
}
