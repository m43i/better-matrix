// A message shown outside its timeline (a pinned or saved message), with the same avatar, text
// scale and row layout as the conversation, and one action at its top right.
import type { AccountId, MessagePreview } from '@better-matrix/matrix-native/types';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { font, identityColor } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { IconButton, Touchable } from '../components';
import type { IconName } from '../icons';
import { usePalette } from '../theme';
import { when } from './labels';

/** The row's action: unpin, or stop saving. */
export interface PreviewAction {
  icon: IconName;
  label: string;
  onPress: () => void;
}

export function PreviewRow({
  account,
  message,
  list,
  place,
  onOpen,
  action,
}: {
  account: AccountId;
  message: MessagePreview;
  /** The list's stable ID, prefixing the row's test IDs (`pinned`, `saved`). */
  list: string;
  /** Where the message was sent, shown after its sender. */
  place?: string;
  onOpen: () => void;
  action?: PreviewAction;
}) {
  const palette = usePalette();
  const body = message.body ?? t('message.unavailable');

  return (
    <View testID={`${list}-${message.event_id}`}>
      <Touchable
        testID={`${list}-open-${message.event_id}`}
        onPress={onOpen}
        disabled={message.body === null}
        accessibilityRole="button"
        accessibilityLabel={message.sender_name ? `${message.sender_name}: ${body}` : body}
        accessibilityHint={t('message.open')}
        style={styles.row}
      >
        <Avatar
          account={account}
          id={message.sender_id}
          name={message.sender_name}
          media={message.sender_avatar}
        />
        <View style={[styles.content, action ? styles.withAction : null]}>
          {message.sender_name || place ? (
            <Text numberOfLines={1}>
              {message.sender_name ? (
                <Text style={[font.bodyStrong, { color: identityColor(message.sender_id) }]}>
                  {message.sender_name}
                </Text>
              ) : null}
              {place ? (
                <Text style={[font.meta, { color: palette.textMuted }]}>
                  {message.sender_name ? '  ' : ''}
                  {place}
                </Text>
              ) : null}
            </Text>
          ) : null}
          <Text
            numberOfLines={4}
            style={[font.body, { color: message.body === null ? palette.textMuted : palette.text }]}
          >
            {body}
          </Text>
          {message.timestamp_ms !== null ? (
            <Text style={[font.meta, { color: palette.textMuted }]}>
              {when(message.timestamp_ms)}
            </Text>
          ) : null}
        </View>
      </Touchable>
      {action ? (
        <View style={styles.action}>
          <IconButton
            icon={action.icon}
            label={action.label}
            testID={`${list}-action-${message.event_id}`}
            size={36}
            color={palette.textMuted}
            onPress={action.onPress}
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
