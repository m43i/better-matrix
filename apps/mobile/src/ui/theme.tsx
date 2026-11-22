// The palette for this device's appearance: the setting, or the system's when it follows it.
// Worked out once at the root (`PaletteProvider`) and handed down: views ask for it everywhere
// (every row, avatar and pill), and one subscription serves them all.

import { createContext, type ReactNode, useContext, useMemo } from 'react';
import { Platform, useColorScheme } from 'react-native';

import { dark, light, type Palette } from '../theme/palette';
import { useStore } from '../state/react';

const PaletteContext = createContext<Palette | null>(null);

/** Follows the appearance setting and the system's scheme for every view under it. */
export function PaletteProvider({ children }: { children: ReactNode }) {
  const appearance = useStore((s) => s.settings.appearance);
  const system = useColorScheme();
  const isDark = appearance === 'system' ? system !== 'light' : appearance === 'dark';

  return (
    <PaletteContext.Provider value={isDark ? dark : light}>{children}</PaletteContext.Provider>
  );
}

export function usePalette(): Palette {
  const palette = useContext(PaletteContext);

  if (!palette) throw new Error('usePalette needs a PaletteProvider');

  return palette;
}

/**
 * Inside a chat bubble: muted words (a quote, a notice) take the shade that stays clear on the
 * bubble.
 */
export function InBubble({ children }: { children: ReactNode }) {
  const palette = usePalette();
  const inside = useMemo(() => ({ ...palette, textMuted: palette.bubbleMeta }), [palette]);

  return <PaletteContext.Provider value={inside}>{children}</PaletteContext.Provider>;
}

/**
 * The colour of a header's buttons and back arrow, as each platform draws them: iOS tints them
 * with the accent, Material draws its navigation arrow and actions in the text colour.
 */
export function headerTint(palette: Palette): string {
  return Platform.OS === 'android' ? palette.text : palette.accent;
}
