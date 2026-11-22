// A thread: its root message on top, the replies under it, and the composer, whose messages
// join the thread. Until the thread's own timeline has its root, the root shows as the room's
// timeline has it.

import { Stack, useLocalSearchParams } from 'expo-router';
import { useMemo } from 'react';
import { Platform, StyleSheet, Text, View } from 'react-native';

import { t } from '../../../../i18n';
import { findMessage } from '../../../../state/messages';
import { useStore } from '../../../../state/react';
import { room as roomOf, timeline } from '../../../../state/selectors';
import { reachParam } from '../../../../state/seek';
import { roomKey, threadKey } from '../../../../state/timeline';
import { inlineName } from '../../../../state/timelineText';
import { font } from '../../../../theme/palette';
import { EmptyState } from '../../../../ui/components';
import { Composer } from '../../../../ui/composer/Composer';
import { usePalette } from '../../../../ui/theme';
import { Conversation } from '../../../../ui/timeline/Conversation';
import { Timeline } from '../../../../ui/timeline/Timeline';

export default function ThreadScreen() {
  const { account, roomId, root, eventId, reach } = useLocalSearchParams<{
    account: string;
    roomId: string;
    root: string;
    eventId?: string;
    reach?: string;
  }>();

  const palette = usePalette();
  const room = useStore((s) => roomOf(s, account, roomId));

  const rootMessage = useStore(
    (s) => findMessage(timeline(s, account, roomKey(roomId)).entries, root) ?? null,
  );

  const key = useMemo(() => threadKey(roomId, root), [roomId, root]);

  if (!room) {
    return (
      <View style={[styles.screen, { backgroundColor: palette.bg }]}>
        <Stack.Screen options={{ title: t('thread.title') }} />
        <EmptyState icon="thread" title={t('mobile.conversation.room_missing')} />
      </View>
    );
  }

  return (
    <>
      <Stack.Screen
        options={{
          title: t('thread.title'),
          headerTitle: () => (
            <View style={styles.title}>
              <Text style={[font.header, { color: palette.text }]}>{t('thread.title')}</Text>
              <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
                {inlineName(room)}
              </Text>
            </View>
          ),
        }}
      />
      <Conversation
        composer={<Composer account={account} timelineKey={key} roomName={inlineName(room)} />}
      >
        <Timeline
          targetReach={reachParam(reach)}
          target={eventId ?? null}
          account={account}
          timelineKey={key}
          root={rootMessage}
        />
      </Conversation>
    </>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  // Centred under iOS's title bar; at the start, as Material's top app bar puts titles.
  title: { alignItems: Platform.OS === 'android' ? 'flex-start' : 'center', maxWidth: 240 },
});
