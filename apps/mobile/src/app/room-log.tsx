// A room's log (from room info): every room event its timeline holds, newest first, the changes
// conversations leave out included, and older history loaded on request (the desktop's room log).

import { Stack, useLocalSearchParams } from 'expo-router';
import { useEffect, useMemo } from 'react';
import { FlatList, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { loadOlder } from '../state/actions/core';
import { releaseTimeline, retainTimeline } from '../state/actions/timeline';
import { atStart } from '../state/plan';
import { useAppStore, useStore } from '../state/react';
import { roomLogOf, timeline as timelineOf } from '../state/selectors';
import { roomKey } from '../state/timeline';
import { font } from '../theme/palette';
import { Button, Divider, EmptyState } from '../ui/components';
import { LogRow } from '../ui/roomLog/LogRow';
import { usePalette } from '../ui/theme';

export default function RoomLogScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const palette = usePalette();
  const key = useMemo(() => roomKey(roomId), [roomId]);
  const log = useStore((s) => roomLogOf(s, account, roomId));
  const reachedStart = useStore((s) => atStart(timelineOf(s, account, key), null));
  const loading = useStore((s) => timelineOf(s, account, key).loadingOlder);
  const error = useStore((s) => timelineOf(s, account, key).historyError);

  // The log reads the room's timeline, which stays open while the log shows.
  useEffect(() => {
    retainTimeline(store, account, key);

    return () => releaseTimeline(store, account, key);
  }, [store, account, key]);

  return (
    <>
      <Stack.Screen options={{ title: t('info.log') }} />
      <FlatList
        testID="room-log"
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={styles.list}
        ItemSeparatorComponent={() => <Divider inset={60} />}
        data={log}
        keyExtractor={(entry) => entry.id}
        renderItem={({ item }) => <LogRow account={account} entry={item} />}
        ListEmptyComponent={<EmptyState icon="history" title={t('info.no_log')} />}
        ListFooterComponent={
          reachedStart ? undefined : (
            <View style={styles.footer}>
              {error !== null ? (
                <Text style={[font.meta, styles.error, { color: palette.danger }]}>{error}</Text>
              ) : null}
              <Button
                testID="log-earlier"
                kind="secondary"
                title={t('info.log_earlier')}
                busy={loading}
                onPress={() => loadOlder(store, account, key)}
              />
            </View>
          )
        }
      />
    </>
  );
}

const styles = StyleSheet.create({
  list: { paddingVertical: 8 },
  footer: { gap: 8, padding: 16 },
  error: { textAlign: 'center' },
});
