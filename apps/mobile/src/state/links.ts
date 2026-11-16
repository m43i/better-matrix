// Links in message text, as matrix-core finds them (packages/matrix-core/src/links.rs): web
// addresses without the punctuation around them, which of them is a GIF (Giphy, Tenor, Imgur, or
// a `.gif` file) and which gets a preview card. Kept in step with the Rust so both apps show the
// same card and the same GIF for a message.

/** Where in `text` its links are, as `[start, end)` offsets, in order. */
export function linkRanges(text: string): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  const word = /\S+/g;

  for (let match = word.exec(text); match !== null; match = word.exec(text)) {
    const { start, link } = unwrapLink(match[0]);

    const rest = link.startsWith('https://')
      ? link.slice(8)
      : link.startsWith('http://')
        ? link.slice(7)
        : null;

    if (rest) ranges.push([match.index + start, match.index + start + link.length]);
  }

  return ranges;
}

/** The links in `text`, in order. */
export function links(text: string): string[] {
  return linkRanges(text).map(([start, end]) => text.slice(start, end));
}

/** Whether `text` is one web address and nothing else. */
export function isLink(text: string): boolean {
  const trimmed = text.trim();
  const first = linkRanges(trimmed)[0];

  return first !== undefined && first[0] === 0 && first[1] === trimmed.length;
}

/**
 * A word without the punctuation around it, and where the rest starts in it. A closing
 * parenthesis stays when the link opened one (`…/wiki/Rust_(programming_language)`).
 */
function unwrapLink(word: string) {
  const opened = word.replace(/^[<("']+/, '');
  let link = opened;

  for (;;) {
    const trimmed = link.replace(/[>"',.!?;:]+$/, '');
    const count = (c: string) => trimmed.split(c).length - 1;
    const next = trimmed.endsWith(')') && count(')') > count('(') ? trimmed.slice(0, -1) : trimmed;

    if (next === link) break;
    link = next;
  }

  return { start: word.length - opened.length, link };
}

/** Where a link's GIF lives: the file itself, or a page whose `og:image` is the GIF. */
export type GifSource = { file: string } | { page: string };

/** The GIF behind a link, if the link is one. */
export function gifSource(link: string): GifSource | null {
  const rest = link.startsWith('https://')
    ? link.slice(8)
    : link.startsWith('http://')
      ? link.slice(7)
      : null;

  if (rest === null) return null;
  const slash = rest.indexOf('/');
  const rawHost = slash < 0 ? rest : rest.slice(0, slash);
  const rawPath = slash < 0 ? '' : rest.slice(slash + 1);
  const path = (rawPath.split(/[?#]/)[0] ?? '').replace(/\/+$/, '');
  const host = rawHost.replace(/^(www\.)+/, '').toLowerCase();
  const last = path.split('/').pop() ?? '';

  switch (host) {
    case 'giphy.com':
      if (path.startsWith('gifs/') || path.startsWith('stickers/')) {
        const id = last.split('-').pop() ?? '';

        return id ? { file: `https://i.giphy.com/${id}.gif` } : null;
      }

      break;
    case 'gph.is':
      return null;
    case 'tenor.com':
      if (path.startsWith('view/')) return { page: `https://tenor.com/${path}` };
      break;
    case 'imgur.com':
      if (path !== '' && !path.includes('/')) return { file: `https://i.imgur.com/${path}.gif` };
      break;
    case 'i.imgur.com':
      if (last.endsWith('.gifv'))
        return { file: `https://i.imgur.com/${last.slice(0, -'.gifv'.length)}.gif` };
      break;
  }

  if (last.toLowerCase().endsWith('.gif')) return { file: link.split(/[?#]/)[0] ?? link };

  return null;
}

/** The first GIF link in a message's text. */
export function findGifLink(text: string): string | null {
  return links(text).find((link) => gifSource(link) !== null) ?? null;
}

/** The first link in a message's text that gets a preview card: one that isn't a GIF. */
export function findPreviewLink(text: string): string | null {
  return links(text).find((link) => gifSource(link) === null) ?? null;
}

/** `text` without the link `url`, its spaces tidied; empty when the text was only the link. */
export function withoutLink(text: string, url: string): string {
  return text
    .split(' ')
    .map((word) => (word.includes(url) ? word.split(url).join('') : word))
    .filter((word) => word !== '')
    .join(' ')
    .trim();
}

/** Matrix HTML without the link `url`: the anchor around it, and the bare address. */
export function htmlWithoutLink(html: string, url: string): string {
  const escaped = url.split('&').join('&amp;');
  let out = html;

  for (const href of [url, escaped]) {
    for (;;) {
      const at = out.indexOf(`href="${href}"`);

      if (at < 0) break;
      const open = out.lastIndexOf('<a', at);
      const close = out.indexOf('</a>', at);

      if (open < 0 || close < 0) break;
      out = out.slice(0, open) + out.slice(close + '</a>'.length);
    }
  }

  return out.split(escaped).join('').split(url).join('').trim();
}
