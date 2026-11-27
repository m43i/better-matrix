// Notices: one message at a time over the top of the screen, gone after a few seconds or a tap.
// iOS presents modals and form sheets in windows of their own above the root view, so there the
// notice sits in a full-window overlay; Android's sheets and modals are views under the root's
// later children, so the notice stays above them as it is.

import { type ReactNode, useEffect } from 'react';
import { Platform, Pressable, StyleSheet, Text } from 'react-native';
import Animated, { FadeInUp, FadeOutUp } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FullWindowOverlay } from 'react-native-screens';

import { dismissNotice } from '../state/actions/core';
import type { Notice as NoticeData } from '../state/types';
import { useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { usePalette } from './theme';

const SHOWN_MS = 5000;

export function Notices() {
  const store = useAppStore();
  const notice = useStore((s) => s.notices[0]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => dismissNotice(store, notice.id), SHOWN_MS);

    return () => clearTimeout(timer);
  }, [store, notice]);

  return <Above>{notice ? <Notice key={notice.id} notice={notice} /> : null}</Above>;
}

/** Over everything: the window's overlay on iOS; on Android, the root's last child is enough. */
function Above({ children }: { children: ReactNode }) {
  return Platform.OS === 'ios' ? <FullWindowOverlay>{children}</FullWindowOverlay> : children;
}

function Notice({ notice }: { notice: NoticeData }) {
  const store = useAppStore();
  const palette = usePalette();
  const insets = useSafeAreaInsets();

  return (
    <Animated.View
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
