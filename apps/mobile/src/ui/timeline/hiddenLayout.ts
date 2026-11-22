// How hidden media's placeholder fits its box (ADR 0011): the icon, what it hides and Show where
// there is room, Show alone in a short box (a wide banner), the icon alone where the caller asks;
// and how far its touch reaches past a box smaller than a finger.

/** The least height that fits the icon, the label and Show without clipping them. */
export const FULL_HEIGHT = 88;

/** The platform's least comfortable tap target: 44 points on iOS, 48 dp on Android. */
export function minTap(os: string): number {
  return os === 'android' ? 48 : 44;
}

/** What the placeholder draws in a box `height` tall: everything, Show alone, or the icon. */
export function hiddenLayout(height: number, compact: boolean): 'full' | 'show' | 'icon' {
  if (compact) return 'icon';

  return height < FULL_HEIGHT ? 'show' : 'full';
}

/** How far a tap reaches past each edge of a box `height` tall, so it is at least `min`. */
export function tapSlop(height: number, min: number): number {
  return height >= min ? 0 : Math.ceil((min - height) / 2);
}
