// An account's avatar, and a dot over a tile's corner when there is something new (the tab
// bar's account, the account menu, the space picker).

import type { AccountSummary } from '@better-matrix/matrix-native/types';
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { Avatar } from '../Avatar';
import { usePalette } from '../theme';

/** A dot over the top right corner of `children`, when `on`. */
export function WithDot({ on, size, children }: { on: boolean; size: number; children: ReactNode }) {
  const palette = usePalette();
  const dot = Math.max(8, size * 0.3);
  return (
    <View style={{ width: size, height: size }}>
      {children}
      {on ? (
        <View
          style={[
            styles.dot,
            { width: dot, height: dot, borderRadius: dot / 2, borderColor: palette.bg, backgroundColor: palette.mention },
          ]}
        />
      ) : null}
    </View>
  );
}

export function AccountAvatar({ summary, size }: { summary: AccountSummary; size: number }) {
  return <Avatar account={summary.id} id={summary.id} name={summary.display_name || summary.id} media={summary.avatar} size={size} />;
}

const styles = StyleSheet.create({
  dot: { position: 'absolute', top: -2, right: -2, borderWidth: 2 },
});
