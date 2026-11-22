// Which spoilers a timeline has shown. A spoiler is known by its message, its place among the
// message's blocks and inlines, and its words: a row the list recycles for another message, or
// an edit that puts other words where a shown spoiler stood, shows its spoilers hidden.
//
// A hidden spoiler's words are never in the drawn text: screen readers read a paragraph's text
// whatever its colours, so a hidden spoiler draws `spoilerCover` and its paragraph reads as
// `hiddenReading` says.

import { type Inline, inlinesText } from './html';

/** The spoilers tapped open on one screen, by `spoilerId`. */
export type Revealed = ReadonlySet<string>;

export const NONE_REVEALED: Revealed = new Set();

/**
 * A spoiler's identity: the message it is in (`message`, its event ID), where in the message it
 * stands (`place`, the path through blocks and inlines) and its words.
 */
export function spoilerId(message: string, place: string, text: string): string {
  return JSON.stringify([message, place, text]);
}

/** The ID of the spoiler `inline`, the `index`th of the inlines at `place` in `message`. */
export function inlineSpoilerId(message: string, place: string, index: number, text: string) {
  return spoilerId(message, `${place}/${index}`, text);
}

/** A blank as wide as a digit that keeps a word together: what a hidden letter draws. */
const COVER = '\u2007';

/**
 * What a hidden spoiler draws in place of `text`: each character a blank of about its width,
 * its spaces and line breaks kept, so the block it fills wraps like the words would. None of
 * its words is in it, for anything that reads the drawn text (a screen reader, a copy).
 */
export function spoilerCover(text: string): string {
  return [...text].map((c) => (/\s/.test(c) ? c : COVER)).join('');
}

/**
 * How a line of `inlines` (at `place` in `message`) reads to a screen reader while one of its
 * spoilers is hidden: its words, each hidden spoiler `label`; `null` when none is hidden, when
 * the drawn text reads as it is.
 */
export function hiddenReading(
  inlines: Inline[],
  message: string,
  place: string,
  revealed: Revealed,
  label: string,
): string | null {
  const hidden = (inline: Inline, index: number) =>
    inline.type === 'spoiler' && !revealed.has(inlineSpoilerId(message, place, index, inline.text));

  if (!inlines.some(hidden)) return null;

  return inlines
    .map((inline, index) => (hidden(inline, index) ? ` ${label} ` : inlinesText([inline])))
    .join('')
    .replace(/ {2,}/g, ' ')
    .trim();
}

/** `revealed` with the spoiler `id` shown too; the same set when it already was. */
export function revealSpoiler(revealed: Revealed, id: string): Revealed {
  if (revealed.has(id)) return revealed;

  return new Set([...revealed, id]);
}
