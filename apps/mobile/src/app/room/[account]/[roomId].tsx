// A room: the native header with its face and name (opening its info) and the call buttons, the
// compact stage while its call runs with the chat chosen, the timeline, who is typing, and the
// composer. An `eventId` scrolls to that message.

import { router, Stack, useLocalSearchParams } from 'expo-router';
import { useMemo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../../i18n';
import { useStore } from '../../../state/react';
import { account as accountOf, room as roomOf } from '../../../state/selectors';
import { roomKey } from '../../../state/timeline';
import { inlineName } from '../../../state/timelineText';
import { routes } from '../../../routes';
import { font } from '../../../theme/palette';
import { SuccessorBanner } from '../../../ui/admin/SuccessorBanner';
import { Avatar } from '../../../ui/Avatar';
import { CallButtons } from '../../../ui/call/CallButtons';
import { CompactCall } from '../../../ui/call/CompactCall';
import { EmptyState } from '../../../ui/components';
import { Composer } from '../../../ui/composer/Composer';
import { usePalette } from '../../../ui/theme';
import { Conversation } from '../../../ui/timeline/Conversation';
import { Timeline } from '../../../ui/timeline/Timeline';
import { TypingLine } from '../../../ui/timeline/TypingLine';

export default function RoomScreen() {
  const { account, roomId, eventId, localTarget } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId?: string;
    localTarget?: string;
  }>();

  const palette = usePalette();
  const room = useStore((s) => roomOf(s, account, roomId));
  const loaded = useStore((s) => accountOf(s, account)?.roomsLoaded ?? true);
  const key = useMemo(() => roomKey(roomId), [roomId]);

  if (!room) {
    return (
      <View style={[styles.screen, { backgroundColor: palette.bg }]}>
        <Stack.Screen options={{ title: '' }} />
        {loaded ? (
          <EmptyState icon="chat" title={t('mobile.conversation.room_missing')} />
        ) : (
          <EmptyState title="" loading />
        )}
      </View>
    );
  }

  return (
    <>
      <Stack.Screen
        options={{
          title: room.name,
          headerTitle: () => (
            <Pressable
              onPress={() => router.push(routes.roomInfo(account, roomId))}
              accessibilityRole="button"
              accessibilityHint={t('room.info')}
              style={styles.title}
              testID="room-title"
            >
              <Avatar
                account={account}
                id={room.room_id}
                name={room.name}
                media={room.avatar}
                size={28}
              />
              <Text
                style={[font.header, styles.titleText, { color: palette.text }]}
                numberOfLines={1}
              >
                {room.name}
              </Text>
            </Pressable>
          ),
          headerRight: () => <CallButtons account={account} roomId={roomId} />,
        }}
      />
      <Conversation
        composer={
          room.successor ? (
            <SuccessorBanner account={account} successor={room.successor} />
          ) : (
            <Composer account={account} timelineKey={key} roomName={inlineName(room)} />
          )
        }
      >
        <CompactCall account={account} roomId={roomId} />
        <Timeline
          localTarget={localTarget === '1'}
          account={account}
          timelineKey={key}
          target={eventId ?? null}
        />
        <TypingLine account={account} roomId={roomId} />
      </Conversation>
    </>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  title: { flexDirection: 'row', alignItems: 'center', gap: 8, maxWidth: 240 },
  titleText: { flexShrink: 1 },
});
