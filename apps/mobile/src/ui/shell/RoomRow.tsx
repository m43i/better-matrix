// One conversation in a list. Compact, as the desktop's sidebar draws it: `#` for a room, the
// other person's picture for a direct chat or a note for the note to self, the name (bold while it wants attention, dimmed when
// muted), the notification mode's icon and a badge for mentions. The detailed form (the DMs tab)
// adds the latest message and its time. Tapping opens it; a long press opens its menu.

import type { AccountId, Message, RoomSummary } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { memo } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { listTime, previewLine } from '../../state/lists';
import { wantsAttention } from '../../state/sections';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Badge, Touchable } from '../components';
import { EmojiText } from '../emoji/EmojiText';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { SearchHighlight } from './SearchHighlight';

/** The leading picture of a compact row, as large as a line of text (the desktop's 18). */
const COMPACT_AVATAR = 22;

/** The leading picture of a detailed row. */
const DETAILED_AVATAR = 32;

export const RoomRow = memo(function RoomRow({
  account,
  room,
  last,
  now,
  detailed = false,
  query,
  onPress,
}: {
  account: AccountId;
  room: RoomSummary;
  /** The room's newest message, for the detailed form's preview line. */
  last: Message | undefined;
  /** When the list was drawn, for the time. */
  now: number;
  /** Show the latest message and its time under the name. */
  detailed?: boolean;
  query?: string;
  /** Instead of opening the room (the search screen closes itself first). */
  onPress?: () => void;
}) {
  const palette = usePalette();
  const attention = wantsAttention(room);
  const muted = room.notifications === 'Mute';
  const direct = room.kind === 'Direct';
  const badge = room.mentions > 0 ? room.mentions : direct && attention ? room.unread : 0;
  const nameColor = muted && !attention ? palette.textMuted : palette.text;

  const status = (
    <>
      {room.notifications === 'MentionsOnly' ? (
        <Icon name="at" size={14} color={palette.textMuted} />
      ) : null}
      {muted ? <Icon name="bellOff" size={14} color={palette.textMuted} /> : null}
    </>
  );

  const name = (
    <Text
      style={[attention ? font.navStrong : font.nav, styles.name, { color: nameColor }]}
      numberOfLines={1}
    >
      {query ? (
        <SearchHighlight text={room.name} query={query} />
      ) : (
        <EmojiText>{room.name}</EmojiText>
      )}
    </Text>
  );

  return (
    <Touchable
      testID={`room-${room.room_id}`}
      accessibilityRole="button"
      accessibilityLabel={room.name}
      accessibilityHint={t('mobile.shell.room_hint')}
      onPress={onPress ?? (() => router.push(routes.room(account, room.room_id)))}
      onLongPress={() => router.push(routes.roomMenu(account, room.room_id))}
      delayLongPress={350}
      style={detailed ? styles.detailed : styles.compact}
    >
      {detailed ? (
        <>
          <Avatar
            account={account}
            id={room.dm_users[0] ?? room.room_id}
            name={room.name}
            media={room.avatar}
            size={DETAILED_AVATAR}
          />
          <View style={styles.text}>
            <View style={styles.line}>
              {name}
              {status}
              {last ? (
                <Text
                  style={[font.meta, { color: attention ? palette.accent : palette.textMuted }]}
                >
                  {listTime(last.timestamp_ms, now)}
                </Text>
              ) : null}
            </View>
            <View style={styles.line}>
              <EmojiText
                style={[
                  font.meta,
                  styles.preview,
                  { color: attention ? palette.text : palette.textMuted },
                ]}
                numberOfLines={1}
              >
                {last ? previewLine(last) : ''}
              </EmojiText>
              <Badge count={badge} mention={room.mentions > 0} />
            </View>
          </View>
        </>
      ) : (
        <>
          <View style={styles.leading}>
            {room.note_to_self ? (
              <Icon name="note" size={18} color={palette.textMuted} />
            ) : direct ? (
              <Avatar
                account={account}
                id={room.dm_users[0] ?? room.room_id}
                name={room.name}
                media={room.avatar}
                size={COMPACT_AVATAR}
              />
            ) : (
              <Text
                style={[
                  font.nav,
                  styles.hash,
                  { color: nameColor === palette.text ? palette.textMuted : nameColor },
                ]}
              >
                #
              </Text>
            )}
          </View>
          {name}
          {status}
          <Badge count={badge} mention={room.mentions > 0} />
        </>
      )}
    </Touchable>
  );
});

const styles = StyleSheet.create({
  compact: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: 16,
    paddingVertical: 9,
    minHeight: 42,
  },
  leading: { width: COMPACT_AVATAR, alignItems: 'center' },
  hash: { fontSize: 18, lineHeight: 22 },
  detailed: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 8,
    minHeight: 52,
  },
  text: { flex: 1, gap: 1 },
  line: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  name: { flex: 1 },
  preview: { flex: 1, fontSize: 13 },
});
