// Which spoilers a timeline has shown. A spoiler is known by its message, its place among the
// message's blocks and inlines, and its words: a row the list recycles for another message, or
// an edit that puts other words where a shown spoiler stood, shows its spoilers hidden.

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

/** `revealed` with the spoiler `id` shown too; the same set when it already was. */
export function revealSpoiler(revealed: Revealed, id: string): Revealed {
  if (revealed.has(id)) return revealed;

  return new Set([...revealed, id]);
}
