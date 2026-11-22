// Hidden media (ADR 0011): a picture, video, linked GIF or link card image the account's media
// choice keeps back shows as a placeholder of its own size with Show. Nothing is fetched before;
// a tap shows that message's media until the app restarts.

import type { Message } from '@better-matrix/matrix-native/types';
import { Platform, Pressable, StyleSheet, Text, View, type ViewStyle } from 'react-native';

import { t } from '../../i18n';
import { revealMedia } from '../../state/actions/safety';
import { useAppStore, useStore } from '../../state/react';
import { mediaHidden } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { hiddenLayout, minTap, tapHeight } from './hiddenLayout';

/** Whether this message's media waits for Show, and the way to show it. */
export function useHiddenMedia(message: Pick<Message, 'sender_id' | 'event_id'>) {
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const hidden = useStore((s) => mediaHidden(s, account, timelineKey.room_id, message));

  return { hidden, reveal: () => revealMedia(store, account, message.event_id) };
}

/**
 * The placeholder: an eye crossed out, what it hides and Show, at the media's size; Show alone
 * in a short box. The press reveals; a box smaller than a finger sits centred in a tap area of
 * the platform's least size.
 */
export function HiddenMedia({
  label,
  onShow,
  style,
  height,
}: {
  /** What is hidden ("Hidden picture"). */
  label: string;
  onShow: () => void;
  style: ViewStyle | ViewStyle[];
  /** The box's height, which decides what fits in it. */
  height: number;
}) {
  const palette = usePalette();
  const layout = hiddenLayout(height);

  return (
    <Pressable
      testID="hidden-media"
      onPress={onShow}
      accessibilityRole="button"
      accessibilityLabel={`${label}. ${t('safety.show_media')}`}
      style={[styles.tap, { minHeight: tapHeight(height, minTap(Platform.OS)) }]}
    >
      <View
        style={[
          style,
          styles.box,
          { backgroundColor: palette.surface, borderColor: palette.divider },
        ]}
      >
        {layout === 'full' ? (
          <>
            <Icon name="eyeOff" size={24} color={palette.textMuted} />
            <Text style={[font.meta, { color: palette.textMuted }]}>{label}</Text>
          </>
        ) : null}
        <Text style={[font.bodyStrong, { color: palette.accent }]} numberOfLines={1}>
          {t('safety.show_media')}
        </Text>
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  tap: { justifyContent: 'center', alignSelf: 'flex-start' },
  box: {
    alignItems: 'center',
    justifyContent: 'center',
    gap: 4,
    borderRadius: 8,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
});
