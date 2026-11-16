// The voice and video call buttons in a room's header (the desktop's `header_buttons`): Join while
// a call is under way in the room, back to the stage while the user is in it, nothing where the
// room allows no call.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { startCall } from '../../state/actions/calls';
import { useAppStore, useStore } from '../../state/react';
import { room as roomOf } from '../../state/selectors';
import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';
import { headerCall } from './stage';
import { useOpenStage } from './useOpenStage';

export function CallButtons({ account, roomId }: { account: AccountId; roomId: string }) {
  const store = useAppStore();
  const openStage = useOpenStage();
  const palette = usePalette();
  const room = useStore((s) => roomOf(s, account, roomId));
  const current = useStore((s) => s.call.current);
  const shown = headerCall(room, current, account);
  const call = (video: boolean) => void startCall(store, account, roomId, video);

  switch (shown.kind) {
    case 'none':
      return null;
    case 'return':
      return <Pill testID="call-return" icon="call" label={t('call.return')} onPress={openStage} />;
    case 'join':
      return (
        <Pill
          testID="call-join"
          icon="call"
          label={t('call.join')}
          hint={t('call.people', { count: shown.count })}
          onPress={() => call(false)}
        />
      );
    case 'start':
      return (
        <View style={styles.row}>
          <IconButton
            testID="call-start"
            icon="call"
            label={t('call.start')}
            color={palette.textMuted}
            onPress={() => call(false)}
          />
          <IconButton
            testID="call-start-video"
            icon="video"
            label={t('call.start_video')}
            color={palette.textMuted}
            onPress={() => call(true)}
          />
        </View>
      );
  }
}

/** A rounded green button with an icon and a label, as the desktop's `pill`. */
function Pill({
  icon,
  label,
  hint,
  onPress,
  testID,
}: {
  icon: IconName;
  label: string;
  hint?: string;
  onPress: () => void;
  testID: string;
}) {
  const palette = usePalette();

  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      hitSlop={6}
      style={({ pressed }) => [
        styles.pill,
        { backgroundColor: palette.success, opacity: pressed ? 0.8 : 1 },
      ]}
    >
      <Icon name={icon} size={16} color={palette.onAccent} />
      <Text style={[font.bodyStrong, { color: palette.onAccent }]}>{label}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: 4 },
  pill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    height: 32,
    paddingHorizontal: 12,
    borderRadius: 16,
  },
});
