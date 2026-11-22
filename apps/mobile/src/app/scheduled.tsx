// A conversation's scheduled messages (sheet, from the bar above its composer): soonest first,
// each with when it sends and what it says (as far as this device can read it), to send now or
// not at all. The server holds them (MSC4140); the list follows every change it reports.

import { useLocalSearchParams } from 'expo-router';
import { useEffect } from 'react';
import { FlatList, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { listScheduled, updateScheduled } from '../state/actions/messages';
import { shallow, useAppStore, useStore } from '../state/react';
import { scheduledIn, sendTimeText } from '../state/schedule';
import { account as accountOf } from '../state/selectors';
import { font } from '../theme/palette';
import { Button, Divider, EmptyState } from '../ui/components';
import { useNow } from '../ui/shell/useNow';
import { usePalette } from '../ui/theme';

export default function ScheduledScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const palette = usePalette();
  const now = useNow();
  const listed = useStore((s) => accountOf(s, account)?.scheduled !== null);
  const messages = useStore((s) => scheduledIn(accountOf(s, account)?.scheduled, roomId), shallow);
  useEffect(() => listScheduled(store, account), [store, account]);

  return (
    <FlatList
      testID="scheduled"
      style={{ backgroundColor: palette.bg }}
      data={messages}
      keyExtractor={(message) => message.delay_id}
      contentContainerStyle={styles.content}
      ItemSeparatorComponent={() => <Divider inset={16} />}
      ListHeaderComponent={
        <Text style={[font.header, styles.title, { color: palette.text }]}>
          {t('mobile.composer.scheduled_title')}
        </Text>
      }
      ListEmptyComponent={
        listed ? (
          <EmptyState icon="scheduleSend" title={t('mobile.composer.scheduled_none')} />
        ) : (
          <EmptyState title="" loading />
        )
      }
      renderItem={({ item }) => (
        <View style={styles.item} testID={`scheduled-${item.delay_id}`}>
          {item.failed === null ? (
            <Text style={[font.meta, { color: palette.accent }]}>
              {t('mobile.composer.scheduled_at', { time: sendTimeText(item.send_at_ms, now) })}
            </Text>
          ) : (
            <Text
              style={[font.meta, { color: palette.danger }]}
              testID={`scheduled-failed-${item.delay_id}`}
            >
              {t('mobile.composer.scheduled_not_sent', { error: item.failed })}
            </Text>
          )}
          <Text
            style={[
              font.body,
              { color: item.body === null ? palette.textMuted : palette.text },
              item.body === null ? styles.unreadable : null,
            ]}
            numberOfLines={4}
          >
            {item.body ?? t('mobile.composer.scheduled_unreadable')}
          </Text>
          <View style={styles.actions}>
            {/* One the server failed to send is done there: it can only be let go. */}
            {item.failed === null ? (
              <View style={styles.action}>
                <Button
                  title={t('mobile.composer.scheduled_send_now')}
                  icon="send"
                  kind="secondary"
                  onPress={() => updateScheduled(store, account, item.delay_id, 'SendNow')}
                  testID={`scheduled-send-${item.delay_id}`}
                />
              </View>
            ) : null}
            <View style={styles.action}>
              <Button
                title={
                  item.failed === null
                    ? t('mobile.composer.scheduled_cancel')
                    : t('mobile.composer.scheduled_dismiss')
                }
                icon="trash"
                kind="plain"
                onPress={() => updateScheduled(store, account, item.delay_id, 'Cancel')}
                testID={`scheduled-cancel-${item.delay_id}`}
              />
            </View>
          </View>
        </View>
      )}
    />
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  title: { paddingHorizontal: 16, paddingTop: 20, paddingBottom: 8 },
  item: { paddingHorizontal: 16, paddingVertical: 12, gap: 6 },
  unreadable: { fontStyle: 'italic' },
  actions: { flexDirection: 'row', gap: 8, marginTop: 4 },
  action: { flex: 1 },
});
