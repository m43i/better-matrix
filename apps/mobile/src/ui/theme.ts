// The palette for this device's appearance: the setting, or the system's when it follows it.

import { useColorScheme } from 'react-native';

import { dark, light, type Palette } from '../theme/palette';
import { useStore } from '../state/react';

export function usePalette(): Palette {
  const appearance = useStore((s) => s.settings.appearance);
  const system = useColorScheme();
  const isDark = appearance === 'system' ? system !== 'light' : appearance === 'dark';

  return isDark ? dark : light;
}
