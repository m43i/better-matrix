// Pinned messages use the same avatar, text scale and row layout as the conversation.
import type { AccountId, PinnedMessage } from '@better-matrix/matrix-native/types';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { font, identityColor } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { IconButton, Touchable } from '../components';
import { usePalette } from '../theme';
import { when } from './labels';

export function PinnedRow({
  account,
  message,
  onOpen,
  onUnpin,
}: {
  account: AccountId;
  message: PinnedMessage;
  onOpen: () => void;
  onUnpin?: () => void;
}) {
  const palette = usePalette();

  return (
    <View testID={`pinned-${message.event_id}`}>
      <Touchable
        testID={`pinned-open-${message.event_id}`}
        onPress={onOpen}
        disabled={message.body === null}
        accessibilityRole="button"
        accessibilityLabel={message.body ?? t('message.unavailable')}
        accessibilityHint={t('message.open')}
        style={styles.row}
      >
        <Avatar
          account={account}
          id={message.sender_id}
          name={message.sender_name}
          media={message.sender_avatar}
        />
        <View style={[styles.content, onUnpin ? styles.withAction : null]}>
          {message.sender_name ? (
            <Text
              numberOfLines={1}
              style={[font.bodyStrong, { color: identityColor(message.sender_id) }]}
            >
              {message.sender_name}
            </Text>
          ) : null}
          <Text
            numberOfLines={4}
            style={[font.body, { color: message.body === null ? palette.textMuted : palette.text }]}
          >
            {message.body ?? t('message.unavailable')}
          </Text>
          {message.timestamp_ms !== null ? (
            <Text style={[font.meta, { color: palette.textMuted }]}>
              {when(message.timestamp_ms)}
            </Text>
          ) : null}
        </View>
      </Touchable>
      {onUnpin ? (
        <View style={styles.action}>
          <IconButton
            icon="unpin"
            label={t('actions.unpin')}
            testID={`pinned-unpin-${message.event_id}`}
            size={36}
            color={palette.textMuted}
            onPress={onUnpin}
          />
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 10,
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
  content: { flex: 1, minWidth: 0, gap: 3 },
  withAction: { paddingRight: 32 },
  action: { position: 'absolute', right: 8, top: 6 },
});
