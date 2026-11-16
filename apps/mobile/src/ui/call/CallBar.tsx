// The strip leading back to the call while its stage isn't shown (the desktop's call bar): where
// the call is, mute and hang up; tapping it opens the stage. Shown at the top of the tab screens.

import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { hangUp, toggleMicrophone } from '../../state/actions/calls';
import { useAppStore, useStore } from '../../state/react';
import { room as roomOf } from '../../state/selectors';
import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { usePalette } from '../theme';
import { useOpenStage } from './useOpenStage';

export function CallBar() {
  const store = useAppStore();
  const openStage = useOpenStage();
  const palette = usePalette();
  const call = useStore((s) => s.call.current);
  const name = useStore((s) => {
    const current = s.call.current;
    if (!current) return null;
    return roomOf(s, current.account, current.room_id)?.name ?? current.room_id;
  });
  if (!call) return null;
  return (
    <Pressable
      testID="call-bar"
      accessibilityRole="button"
      accessibilityLabel={t('call.return')}
      onPress={openStage}
      style={[styles.bar, { backgroundColor: `${palette.success}29`, borderColor: palette.divider }]}
    >
      <View style={[styles.dot, { backgroundColor: palette.success }]} />
      <View style={styles.text}>
        <Text style={[font.navStrong, { color: palette.text }]}>{t('call.in_call')}</Text>
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
          {name}
        </Text>
      </View>
      <IconButton
        testID="call-bar-mic"
        icon={call.microphone ? 'mic' : 'micOff'}
        label={call.microphone ? t('call.mute') : t('call.unmute')}
        color={call.microphone ? palette.text : palette.danger}
        size={36}
        onPress={() => void toggleMicrophone(store)}
      />
      <IconButton
        testID="call-bar-hang-up"
        icon="hangUp"
        label={t('call.hang_up')}
        color={palette.danger}
        size={36}
        onPress={() => void hangUp(store)}
      />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  dot: { width: 8, height: 8, borderRadius: 4 },
  text: { flex: 1, minWidth: 0 },
});
