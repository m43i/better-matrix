// Hidden media (ADR 0011): a picture, video, linked GIF or link card image the account's media
// choice keeps back shows as a placeholder of its own size with Show. Nothing is fetched before;
// a tap shows that message's media until the app restarts.

import type { Message } from '@better-matrix/matrix-native/types';
import { Platform, Pressable, StyleSheet, Text, type ViewStyle } from 'react-native';

import { t } from '../../i18n';
import { revealMedia } from '../../state/actions/safety';
import { useAppStore, useStore } from '../../state/react';
import { mediaHidden } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { hiddenLayout, minTap, tapSlop } from './hiddenLayout';

/** Whether this message's media waits for Show, and the way to show it. */
export function useHiddenMedia(message: Pick<Message, 'sender_id' | 'event_id'>) {
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const hidden = useStore((s) => mediaHidden(s, account, timelineKey.room_id, message));

  return { hidden, reveal: () => revealMedia(store, account, message.event_id) };
}

/** The placeholder: an eye crossed out, what it hides and Show, at the media's size. */
export function HiddenMedia({
  label,
  onShow,
  style,
  height,
  compact = false,
}: {
  /** What is hidden ("Hidden picture"). */
  label: string;
  onShow: () => void;
  style: ViewStyle | ViewStyle[];
  /** The box's height, which decides what fits in it. */
  height: number;
  /** Too small for words (a link card's picture): the icon alone. */
  compact?: boolean;
}) {
  const palette = usePalette();
  // A short box (a wide banner) fits Show alone; a box smaller than a finger still takes one.
  const layout = hiddenLayout(height, compact);
  const slop = tapSlop(height, minTap(Platform.OS));

  return (
    <Pressable
      testID="hidden-media"
      onPress={onShow}
      hitSlop={slop}
      accessibilityRole="button"
      accessibilityLabel={`${label}. ${t('safety.show_media')}`}
      style={[
        style,
        styles.box,
        { backgroundColor: palette.surface, borderColor: palette.divider },
      ]}
    >
      {layout === 'show' ? null : (
        <Icon name="eyeOff" size={layout === 'icon' ? 20 : 24} color={palette.textMuted} />
      )}
      {layout === 'full' ? (
        <Text style={[font.meta, { color: palette.textMuted }]}>{label}</Text>
      ) : null}
      {layout === 'icon' ? null : (
        <Text style={[font.bodyStrong, { color: palette.accent }]} numberOfLines={1}>
          {t('safety.show_media')}
        </Text>
      )}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  box: {
    alignItems: 'center',
    justifyContent: 'center',
    gap: 4,
    borderRadius: 8,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
});
