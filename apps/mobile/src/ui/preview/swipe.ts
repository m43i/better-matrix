// Swiping sideways through a preview's gallery: a decisive horizontal swipe steps to the next
// item (leftwards) or the one before (rightwards); vertical drags and small wobbles do nothing.

import { usePanGesture } from 'react-native-gesture-handler';

/** How far a finger moves sideways before the swipe takes the gesture. */
const SLOP = 24;

/** How far, or how fast, a swipe goes to count. */
const DISTANCE = 64;

const VELOCITY = 600;

/**
 * A pan that calls `onSwipe` with +1 (next) or -1 (previous) when let go far or fast enough;
 * `null` turns it off. `canSwipe` is asked at the end, so a zoomed-in image can keep its pan.
 * `bottomInset` leaves that much of the view's bottom to its own controls (a video's scrubber).
 */
export function useSwipe(
  onSwipe: ((step: 1 | -1) => void) | null,
  { canSwipe, bottomInset = 0 }: { canSwipe?: () => boolean; bottomInset?: number } = {},
) {
  return usePanGesture({
    enabled: onSwipe !== null,
    runOnJS: true,
    // One finger: a pinch that ends back at 1× isn't a swipe.
    maxPointers: 1,
    activeOffsetX: [-SLOP, SLOP],
    failOffsetY: [-SLOP, SLOP],
    hitSlop: bottomInset > 0 ? { bottom: -bottomInset } : undefined,
    onDeactivate: (event) => {
      if (event.canceled || !onSwipe || (canSwipe && !canSwipe())) return;
      const { translationX, velocityX } = event;

      const far = Math.abs(translationX) >= DISTANCE;

      if (!far && Math.abs(velocityX) < VELOCITY) return;
      onSwipe((far ? translationX : velocityX) < 0 ? 1 : -1);
    },
  });
}
