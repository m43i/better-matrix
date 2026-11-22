// Hidden media (ADR 0011): a picture, video, linked GIF or link card image the account's media
// choice keeps back shows as a placeholder of its own size with Show. Nothing is fetched before;
// a tap shows that message's media until the app restarts.

import type { Message } from '@better-matrix/matrix-native/types';
import { Pressable, StyleSheet, Text, type ViewStyle } from 'react-native';

import { t } from '../../i18n';
import { revealMedia } from '../../state/actions/safety';
import { useAppStore, useStore } from '../../state/react';
import { mediaHidden } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';

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
  compact = false,
}: {
  /** What is hidden ("Hidden picture"). */
  label: string;
  onShow: () => void;
  style: ViewStyle | ViewStyle[];
  /** Too small for words (a link card's picture): the icon alone. */
  compact?: boolean;
}) {
  const palette = usePalette();

  return (
    <Pressable
      testID="hidden-media"
      onPress={onShow}
      accessibilityRole="button"
      accessibilityLabel={`${label}. ${t('mobile.safety.show')}`}
      style={[
        style,
        styles.box,
        { backgroundColor: palette.surface, borderColor: palette.divider },
      ]}
    >
      <Icon name="eyeOff" size={compact ? 20 : 24} color={palette.textMuted} />
      {compact ? null : (
        <>
          <Text style={[font.meta, { color: palette.textMuted }]}>{label}</Text>
          <Text style={[font.bodyStrong, { color: palette.accent }]}>
            {t('mobile.safety.show')}
          </Text>
        </>
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
