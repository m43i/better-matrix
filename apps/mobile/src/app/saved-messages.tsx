// The messages the account saved, across its rooms, most recently saved first: from the account
// menu. A row opens its message where it was sent; its bookmark stops saving it.
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { FlatList, View } from 'react-native';
import { t } from '../i18n';
import { routes } from '../routes';
import { listSaved, setSaved } from '../state/actions/message-actions';
import { shallow, useAppStore, useStore } from '../state/react';
import { account as accountOf, savedMessages, savedSignature } from '../state/selectors';
import { Divider, EmptyState, ListRow } from '../ui/components';
import { PreviewRow } from '../ui/people/PreviewRow';

export default function SavedMessagesScreen() {
  const { account } = useLocalSearchParams<{ account: string }>();
  const store = useAppStore();
  const router = useRouter();
  const saved = useStore((s) => savedMessages(s, account), shallow);
  const signature = useStore((s) => savedSignature(s, account));

  const roomNames = useStore(
    (s) => Object.fromEntries((accountOf(s, account)?.rooms ?? []).map((r) => [r.room_id, r.name])),
    shallow,
  );

  const [retry, setRetry] = useState(0);

  const [status, setStatus] = useState<{ loading: boolean; error: string | null }>({
    loading: true,
    error: null,
  });

  // One list request at a time, in order: a newer list never gives way to an older one that
  // happened to answer last.
  const listing = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    let active = true;
    const listed = listing.current.catch(() => undefined).then(() => listSaved(store, account));
    listing.current = listed;

    void listed.then(
      () => {
        if (active) setStatus({ loading: false, error: null });
      },
      (error: Error) => {
        if (active) setStatus({ loading: false, error: error.message });
      },
    );

    return () => {
      active = false;
    };
  }, [store, account, signature, retry]);

  const reload = () => {
    setStatus({ loading: true, error: null });
    setRetry((n) => n + 1);
  };

  return (
    <>
      <Stack.Screen options={{ title: t('saved.title') }} />
      <FlatList
        testID="saved-messages"
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={{ paddingVertical: 8 }}
        ItemSeparatorComponent={() => <Divider inset={62} />}
        data={saved ?? []}
        keyExtractor={(item) => `${item.room_id}|${item.preview.event_id}`}
        ListHeaderComponent={
          status.error ? (
            <View>
              <EmptyState icon="alert" title={t('saved.failed', { error: status.error })} />
              <ListRow title={t('saved.retry')} onPress={reload} testID="saved-retry" />
            </View>
          ) : undefined
        }
        ListEmptyComponent={
          !status.error ? (
            <EmptyState
              icon="bookmark"
              title={t(status.loading && saved === undefined ? 'info.loading' : 'saved.empty')}
            />
          ) : undefined
        }
        refreshing={status.loading}
        onRefresh={reload}
        renderItem={({ item }) => (
          <PreviewRow
            account={account}
            message={item.preview}
            list="saved"
            place={t('saved.in_room', { room: roomNames[item.room_id] ?? item.room_id })}
            onOpen={() => {
              router.push(
                item.preview.thread
                  ? routes.thread(account, item.room_id, item.preview.thread, item.preview.event_id)
                  : routes.room(account, item.room_id, item.preview.event_id),
              );
            }}
            action={{
              icon: 'unbookmark',
              label: t('actions.unsave'),
              onPress: () => setSaved(store, account, item.room_id, item.preview.event_id, false),
            }}
          />
        )}
      />
    </>
  );
}
