// Whether a screen is still the one on show: work it started that ends after the user left it
// (a join, a chat being opened) must not navigate them away from where they went.

import { useFocusEffect } from 'expo-router';
import { type RefObject, useCallback, useRef } from 'react';

/** A ref that is true while the screen is focused, false once it lost focus or went away. */
export function useShown(): RefObject<boolean> {
  const shown = useRef(true);
  useFocusEffect(
    useCallback(() => {
      shown.current = true;
      return () => {
        shown.current = false;
      };
    }, []),
  );
  return shown;
}
