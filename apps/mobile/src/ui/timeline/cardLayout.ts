// Where a link card puts the page's picture: beside its words where they keep a readable width,
// else under them, so the picture (or, held back, its Show) is never left out (ADR 0011).

/** The picture's side. */
export const CARD_IMAGE_SIDE = 64;

/** The least width a link card keeps for its words beside its picture. */
export const CARD_WORDS_MIN = 100;

/** The card's padding, accent bar and gaps around its words, beside a picture. */
const CARD_CHROME = 2 * 12 + 3 + 2 * 12;

/** Where the picture goes in a card `width` wide. */
export function cardLayout(width: number): 'beside' | 'below' {
  return width - CARD_CHROME - CARD_IMAGE_SIDE >= CARD_WORDS_MIN ? 'beside' : 'below';
}
