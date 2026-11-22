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
import { useMessageSide, useTimelineContext } from './context';
import { hiddenLayout, minTap, tapArea } from './hiddenLayout';

/** Whether this message's media waits for Show, and the way to show it. */
export function useHiddenMedia(message: Pick<Message, 'sender_id' | 'event_id'>) {
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const hidden = useStore((s) => mediaHidden(s, account, timelineKey.room_id, message));

  return { hidden, reveal: () => revealMedia(store, account, message.event_id) };
}

/**
 * The placeholder: an eye crossed out, what it hides and Show, at the media's size; Show alone
 * in a short box, and under a box too small for it. The press reveals, a long press opens the
 * message's actions; the tap area is never smaller than a finger on either axis.
 */
export function HiddenMedia({
  label,
  onShow,
  onLongPress,
  style,
  width,
  height,
  inCard = false,
}: {
  /** What is hidden ("Hidden picture"). */
  label: string;
  onShow: () => void;
  onLongPress?: () => void;
  style: ViewStyle | ViewStyle[];
  /** The box's size, which decides what fits in it. */
  width: number;
  height: number;
  /** Laid out in a link card's row, at its top, rather than on the message's side. */
  inCard?: boolean;
}) {
  const palette = usePalette();
  const layout = hiddenLayout(width, height);
  // On the message's side, as the media it stands for (chat bubbles keep the user's own right).
  const side = useMessageSide();
  const iconSize = Math.min(24, Math.min(width, height) - 8);

  const show = (
    <Text style={[font.bodyStrong, { color: palette.accent }]} numberOfLines={1}>
      {t('safety.show_media')}
    </Text>
  );

  return (
    <Pressable
      testID="hidden-media"
      onPress={onShow}
      onLongPress={onLongPress}
      delayLongPress={350}
      accessibilityRole="button"
      accessibilityLabel={`${label}. ${t('safety.show_media')}`}
      style={[
        styles.tap,
        { alignSelf: inCard ? 'flex-start' : side },
        tapArea(width, height, minTap(Platform.OS)),
      ]}
    >
      <View
        style={[
          style,
          styles.box,
          { backgroundColor: palette.surface, borderColor: palette.divider },
        ]}
      >
        {layout === 'show' || iconSize < 12 ? null : (
          <Icon name="eyeOff" size={iconSize} color={palette.textMuted} />
        )}
        {layout === 'full' ? (
          <Text style={[font.meta, { color: palette.textMuted }]}>{label}</Text>
        ) : null}
        {layout === 'outside' ? null : show}
      </View>
      {layout === 'outside' ? show : null}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  tap: { alignItems: 'center', justifyContent: 'center', gap: 4 },
  box: {
    alignItems: 'center',
    justifyContent: 'center',
    gap: 4,
    borderRadius: 8,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
});
