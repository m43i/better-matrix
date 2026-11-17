// Pins are room state, so this list also fetches messages outside the loaded timeline.
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { FlatList, View } from 'react-native';
import { t } from '../i18n';
import { routes } from '../routes';
import { listPins, setPinned } from '../state/actions/message-actions';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf, room as roomOf } from '../state/selectors';
import { Divider, EmptyState, ListRow } from '../ui/components';
import { PinnedRow } from '../ui/people/PinnedRow';

export default function PinnedMessagesScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const room = useStore((s) => roomOf(s, account, roomId));
  const pins = useStore((s) => accountOf(s, account)?.pins[roomId]);
  const signature = JSON.stringify(room?.pinned ?? []);
  const [retry, setRetry] = useState(0);

  const [status, setStatus] = useState<{ loading: boolean; error: string | null }>({
    loading: true,
    error: null,
  });

  useEffect(() => {
    let active = true;

    void listPins(store, account, roomId).then(
      () => {
        if (active) setStatus({ loading: false, error: null });
      },
      (error: Error) => {
        if (active) setStatus({ loading: false, error: String(error) });
      },
    );

    return () => {
      active = false;
    };
  }, [store, account, roomId, signature, retry]);

  return (
    <>
      <Stack.Screen options={{ title: t('info.pins') }} />
      <FlatList
        testID="pinned-messages"
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{ paddingVertical: 8 }}
        ItemSeparatorComponent={() => <Divider inset={62} />}
        data={(pins ?? []).filter((pin) => room?.pinned.includes(pin.event_id))}
        keyExtractor={(pin) => pin.event_id}
        ListHeaderComponent={
          status.error ? (
            <View>
              <EmptyState icon="alert" title={status.error} />
              <ListRow
                title={t('info.pins_retry')}
                onPress={() => {
                  setStatus({ loading: true, error: null });
                  setRetry((n) => n + 1);
                }}
                testID="pins-retry"
              />
            </View>
          ) : undefined
        }
        ListEmptyComponent={
          !status.error ? (
            <EmptyState icon="pin" title={t(status.loading ? 'info.loading' : 'info.no_pins')} />
          ) : undefined
        }
        refreshing={status.loading}
        onRefresh={() => {
          setStatus({ loading: true, error: null });
          setRetry((n) => n + 1);
        }}
        renderItem={({ item }) => (
          <PinnedRow
            account={account}
            message={item}
            onOpen={() => {
              router.push(
                item.thread
                  ? routes.thread(account, roomId, item.thread, item.event_id)
                  : routes.room(account, roomId, item.event_id),
              );
            }}
            onUnpin={
              room?.can_pin
                ? () => setPinned(store, account, roomId, item.event_id, false)
                : undefined
            }
          />
        )}
      />
    </>
  );
}
