// The palette for this device's appearance: the setting, or the system's when it follows it.

import { Platform, useColorScheme } from 'react-native';

import { dark, light, type Palette } from '../theme/palette';
import { useStore } from '../state/react';

export function usePalette(): Palette {
  const appearance = useStore((s) => s.settings.appearance);
  const system = useColorScheme();
  const isDark = appearance === 'system' ? system !== 'light' : appearance === 'dark';

  return isDark ? dark : light;
}

/**
 * The colour of a header's buttons and back arrow, as each platform draws them: iOS tints them
 * with the accent, Material draws its navigation arrow and actions in the text colour.
 */
export function headerTint(palette: Palette): string {
  return Platform.OS === 'android' ? palette.text : palette.accent;
}
