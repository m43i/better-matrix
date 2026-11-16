// Notices: one message at a time over the top of the screen, gone after a few seconds or a tap.

import { useEffect } from 'react';
import { Pressable, StyleSheet, Text } from 'react-native';
import Animated, { FadeInUp, FadeOutUp } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { dismissNotice } from '../state/actions/core';
import { useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { usePalette } from './theme';

const SHOWN_MS = 5000;

export function Notices() {
  const store = useAppStore();
  const notice = useStore((s) => s.notices[0]);
  const palette = usePalette();
  const insets = useSafeAreaInsets();
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => dismissNotice(store, notice.id), SHOWN_MS);

    return () => clearTimeout(timer);
  }, [store, notice]);

  if (!notice) return null;

  return (
    <Animated.View
      key={notice.id}
      entering={FadeInUp}
      exiting={FadeOutUp}
      style={[styles.wrap, { top: insets.top + 8 }]}
      pointerEvents="box-none"
    >
      <Pressable
        accessibilityRole="alert"
        onPress={() => dismissNotice(store, notice.id)}
        style={[styles.notice, { backgroundColor: palette.dark ? palette.surface : '#262626' }]}
      >
        <Text style={[font.body, { color: '#f5f5f5' }]}>{notice.message}</Text>
      </Pressable>
    </Animated.View>
  );
}

const styles = StyleSheet.create({
  wrap: { position: 'absolute', left: 12, right: 12, zIndex: 100 },
  notice: {
    borderRadius: 12,
    paddingHorizontal: 16,
    paddingVertical: 12,
    shadowColor: '#000',
    shadowOpacity: 0.25,
    shadowRadius: 12,
    shadowOffset: { width: 0, height: 4 },
    elevation: 6,
  },
});
