// How hidden media's placeholder fits its box (ADR 0011): the icon, what it hides and Show where
// there is room; Show alone in a short box (a wide banner); and for a box too small for a word
// (a link card's picture, a sticker), Show laid out beside it in the tap area, which is never
// smaller than a finger on either axis.

/** The least box that fits the icon, the label and Show without clipping them. */
export const FULL_SIDE = 88;

/** The least box that fits Show on one line in any of the app's languages. */
export const SHOW_WIDTH = 96;

/** The least height that fits Show on one line. */
export const SHOW_HEIGHT = 32;

/** The platform's least comfortable tap target: 44 points on iOS, 48 dp on Android. */
export function minTap(os: string): number {
  return os === 'android' ? 48 : 44;
}

/**
 * What the placeholder draws for a box `width` by `height`: everything in it, Show alone in it,
 * or Show under it (`outside`), where it would clip.
 */
export function hiddenLayout(width: number, height: number): 'full' | 'show' | 'outside' {
  if (width >= FULL_SIDE && height >= FULL_SIDE) return 'full';

  return width >= SHOW_WIDTH && height >= SHOW_HEIGHT ? 'show' : 'outside';
}

/** The least width and height of the tap area around a box: the box, or `min` if smaller. */
export function tapArea(width: number, height: number, min: number) {
  return { minWidth: Math.max(width, min), minHeight: Math.max(height, min) };
}
