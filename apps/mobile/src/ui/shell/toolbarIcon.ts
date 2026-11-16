// Icons for the native header's toolbar buttons and menus (`Stack.Toolbar`), which take an SF
// Symbol name on iOS but an image on Android: the Material Symbol is drawn to one once.

import type { SFSymbol } from 'expo-symbols';
import { unstable_getMaterialSymbolSourceAsync } from 'expo-symbols';
import { useEffect, useState } from 'react';
import { type ImageSourcePropType, Platform } from 'react-native';

import { type IconName, icons } from '../icons';

/** Drawn Material Symbols, by name and colour, shared by every header. */
const drawn = new Map<string, ImageSourcePropType>();

/** The toolbar icon for `name`: the SF Symbol on iOS, a drawn Material Symbol on Android. */
export function useToolbarIcon(
  name: IconName,
  color: string,
): SFSymbol | ImageSourcePropType | undefined {
  const key = `${name}|${color}`;
  const [loaded, setLoaded] = useState<{ key: string; image: ImageSourcePropType } | null>(null);
  useEffect(() => {
    if (Platform.OS !== 'android' || drawn.has(key)) return;

    let live = true;
    void unstable_getMaterialSymbolSourceAsync(icons[name].android, 24, color).then((image) => {
      if (!image) return;
      drawn.set(key, image);

      if (live) setLoaded({ key, image });
    });

    return () => {
      live = false;
    };
  }, [key, name, color]);

  return Platform.OS === 'ios'
    ? icons[name].ios
    : (drawn.get(key) ?? (loaded?.key === key ? loaded.image : undefined));
}
