// The captured runs are emoji. A bare text symbol, a digit without its keycap, and an
// explicit text presentation (VS15) stay in the surrounding font.
const base = String.raw`(?:\p{Emoji_Presentation}(?!\uFE0E)\uFE0F?|\p{Emoji}\uFE0F)\p{Emoji_Modifier}?`;

const pictograph = String.raw`\p{Extended_Pictographic}\uFE0F?\p{Emoji_Modifier}?`;

const joined = String.raw`${pictograph}(?:\u200D${pictograph})+`;

const pattern = new RegExp(
  String.raw`(\p{Regional_Indicator}{2}|[0-9#*]\uFE0F?\u20E3|${joined}|${base}(?:[\u{E0020}-\u{E007E}]+\u{E007F})?)`,
  'u',
);

/** Plain and emoji text alternate, starting with plain text (possibly empty). */
export function emojiRuns(text: string): string[] {
  return text.split(pattern);
}
