// An account's avatar, and a dot over a tile's corner when there is something new (the header's
// profile button, the account menu, the space picker); the user's status as a dot, as the
// desktop's rail shows it.

import type { AccountSummary, Status } from '@better-matrix/matrix-native/types';
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { Avatar } from '../Avatar';
import { usePalette } from '../theme';

/** A dot over the top right corner of `children`, when `on`. */
export function WithDot({
  on,
  size,
  children,
}: {
  on: boolean;
  size: number;
  children: ReactNode;
}) {
  const palette = usePalette();
  const dot = Math.max(8, size * 0.3);

  return (
    <View style={{ width: size, height: size }}>
      {children}
      {on ? (
        <View
          style={[
            styles.dot,
            {
              width: dot,
              height: dot,
              borderRadius: dot / 2,
              borderColor: palette.bg,
              backgroundColor: palette.mention,
            },
          ]}
        />
      ) : null}
    </View>
  );
}

/**
 * The user's status: filled green when online, amber when away, a hollow ring when hidden (as
 * others see them: offline). `ring` is the colour of the surface it sits on.
 */
export function StatusDot({ status, size, ring }: { status: Status; size: number; ring: string }) {
  const palette = usePalette();
  const fill = { Online: palette.success, Away: palette.warning, Hidden: ring }[status];
  const border = status === 'Hidden' ? palette.textMuted : ring;

  return (
    <View
      style={{
        width: size,
        height: size,
        borderRadius: size / 2,
        borderWidth: 2,
        borderColor: border,
        backgroundColor: fill,
      }}
    />
  );
}

/** An account's picture; with `status`, the user's status over its bottom right corner. */
export function AccountAvatar({
  summary,
  size,
  status,
}: {
  summary: AccountSummary;
  size: number;
  status?: Status;
}) {
  const palette = usePalette();

  return (
    <View style={{ width: size, height: size }}>
      <Avatar
        account={summary.id}
        id={summary.id}
        name={summary.display_name || summary.id}
        media={summary.avatar}
        size={size}
      />
      {status ? (
        <View style={styles.status}>
          <StatusDot status={status} size={Math.max(10, size * 0.36)} ring={palette.bg} />
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  dot: { position: 'absolute', top: -2, right: -2, borderWidth: 2 },
  status: { position: 'absolute', right: 0, bottom: 0 },
});
