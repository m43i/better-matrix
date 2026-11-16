// A conversation's layout, for rooms and threads: the timeline (and what goes under it) with the
// composer at the bottom, kept above the keyboard. The home indicator's room is left free while
// the keyboard is down.

import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';
import { KeyboardAvoidingView, useKeyboardState } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { usePalette } from '../theme';

export function Conversation({ children, composer }: { children: ReactNode; composer: ReactNode }) {
  const palette = usePalette();
  const insets = useSafeAreaInsets();
  const keyboard = useKeyboardState((s) => s.isVisible);
  return (
    <KeyboardAvoidingView behavior="padding" automaticOffset style={[styles.screen, { backgroundColor: palette.bg }]}>
      {children}
      {composer}
      <View style={{ height: keyboard ? 0 : insets.bottom }} />
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
});
