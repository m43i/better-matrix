// Activity (the bell in Home's header): what notified the user, newest first, for every signed-in account (each row says
// whose when there are several). A row opens the room at that message.

import { router, Stack } from 'expo-router';
import { useEffect, useMemo } from 'react';
import { FlatList, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { markActivitySeen, setActive } from '../state/actions/core';
import { type ActivityRow, activityRows } from '../state/lists';
import { useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { EmptyState, Touchable } from '../ui/components';
import { usePalette } from '../ui/theme';

export default function ActivityScreen() {
  const activity = useStore((s) => s.activity);
  const accounts = useStore((s) => s.accounts);
  const active = useStore((s) => s.active);
  // Rows change only with the notifications or the accounts (names, rooms).
  const rows = useMemo(() => activityRows({ activity, accounts }), [activity, accounts]);
  const store = useAppStore();
  // Everything listed here counts as seen, also what arrives while it is open.
  useEffect(() => markActivitySeen(store), [store, activity]);

  return (
    <>
      <Stack.Screen options={{ title: t('mobile.tabs.activity') }} />
      <FlatList
        testID="activity"
        style={styles.fill}
        contentInsetAdjustmentBehavior="automatic"
        data={rows}
        keyExtractor={(row) => `${row.notification.account}|${row.notification.event_id}`}
        renderItem={({ item }) => <Row row={item} active={active} />}
        contentContainerStyle={rows.length === 0 ? styles.fill : undefined}
        ListEmptyComponent={
          <EmptyState
            icon="bell"
            title={t('overview.caught_up')}
            body={t('mobile.shell.activity_empty')}
          />
        }
      />
    </>
  );
}

function Row({ row, active }: { row: ActivityRow; active: string | null }) {
  const store = useAppStore();
  const palette = usePalette();
  const { notification: n, room } = row;
  const direct = room?.kind === 'Direct';

  // In a direct chat the room is named after the sender: say it once.
  const body =
    direct && n.sender_name === n.room_name
      ? n.body
      : t('overview.preview', { sender: n.sender_name, body: n.body });

  return (
    <Touchable
      testID={`activity-${n.event_id}`}
      accessibilityRole="button"
      onPress={() => {
        if (n.account !== active) setActive(store, n.account);
        router.push(routes.room(n.account, n.room_id, n.event_id));
      }}
      style={styles.row}
    >
      <Avatar
        account={n.account}
        id={room?.dm_users[0] ?? n.room_id}
        name={n.room_name}
        media={room?.avatar}
        size={40}
      />
      <View style={styles.text}>
        <View style={styles.line}>
          <Text style={[font.navStrong, styles.grow, { color: palette.text }]} numberOfLines={1}>
            {n.room_name}
          </Text>
          {row.accountLabel ? (
            <Text style={[font.meta, styles.label, { color: palette.textMuted }]} numberOfLines={1}>
              {row.accountLabel}
            </Text>
          ) : null}
        </View>
        <Text style={[font.body, { color: palette.textMuted }]} numberOfLines={2}>
          {body}
        </Text>
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  fill: { flexGrow: 1 },
  row: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  text: { flex: 1, gap: 2 },
  line: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  grow: { flex: 1 },
  label: { maxWidth: '40%' },
});
