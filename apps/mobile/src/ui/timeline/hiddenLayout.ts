// How hidden media's placeholder fits its box (ADR 0011): the icon, what it hides and Show where
// there is room, Show alone in a short box (a wide banner or a link card's picture); and the
// tap area a box smaller than a finger sits in.

/** The least height that fits the icon, the label and Show without clipping them. */
export const FULL_HEIGHT = 88;

/** The platform's least comfortable tap target: 44 points on iOS, 48 dp on Android. */
export function minTap(os: string): number {
  return os === 'android' ? 48 : 44;
}

/** What the placeholder draws in a box `height` tall: everything, or Show alone. */
export function hiddenLayout(height: number): 'full' | 'show' {
  return height < FULL_HEIGHT ? 'show' : 'full';
}

/**
 * How tall the tap area around a box `height` tall is: the box, or `min` around a smaller one
 * (a touch can't reach past its parent, so the area is laid out, not stretched).
 */
export function tapHeight(height: number, min: number): number {
  return Math.max(height, min);
}
