import { useFonts } from 'expo-font';
import { createContext, useContext, type ReactNode } from 'react';
import { Text, type TextProps } from 'react-native';

import { useStore } from '../../state/react';
import { emojiRuns } from './emojiRuns';

const families = { color: 'BetterMatrixColorEmoji', monochrome: 'BetterMatrixNotoEmoji' };

const fonts = {
  [families.color]:
    process.env.EXPO_OS === 'ios'
      ? require('../../../../../assets/emoji/BetterMatrixColorEmojiSbix.ttf')
      : require('../../../../../assets/emoji/NotoColorEmoji.ttf'),
  [families.monochrome]:
    process.env.EXPO_OS === 'ios'
      ? require('../../../../../assets/emoji/BetterMatrixNotoEmojiSbix.ttf')
      : require('../../../../../assets/emoji/BetterMatrixNotoEmoji.ttf'),
};

const EmojiFamily = createContext<string | undefined>(undefined);

export function EmojiFontProvider({ children }: { children: ReactNode }) {
  const [loaded] = useFonts(fonts);
  const style = useStore((s) => s.settings.emojiStyle);

  return (
    <EmojiFamily.Provider value={loaded ? families[style] : undefined}>
      {children}
    </EmojiFamily.Provider>
  );
}

/** Native text with the selected font on emoji runs, including inside richer text. */
export function EmojiText({
  children,
  ...props
}: Omit<TextProps, 'children'> & { children: string }) {
  const fontFamily = useContext(EmojiFamily);
  const runs = fontFamily ? emojiRuns(children) : [children];

  return (
    <Text {...props}>
      {runs.length === 1
        ? children
        : runs.map((run, index) =>
            index % 2 === 0 ? (
              run
            ) : (
              <Text key={index} style={{ fontFamily, fontWeight: '400', fontStyle: 'normal' }}>
                {run}
              </Text>
            ),
          )}
    </Text>
  );
}
