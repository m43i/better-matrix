// Choose a joined conversation on the message's account; failures leave the picker open.
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { FlatList, TextInput, View } from 'react-native';
import { t } from '../i18n';
import { forwardMessage } from '../state/actions/message-actions';
import { matches } from '../state/people';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf } from '../state/selectors';
import { roomKey, threadKey } from '../state/timeline';
import { ListRow, EmptyState } from '../ui/components';
import { usePalette } from '../ui/theme';
import { useShown } from '../ui/useShown';

export default function ForwardMessageScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const shown = useShown();
  const rooms = useStore((s) => accountOf(s, account)?.rooms);
  const [search, setSearch] = useState('');
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const forward = async (destination: string) => {
    if (sending) return;
    setSending(true);
    setError(null);

    try {
      await forwardMessage(
        store,
        account,
        thread ? threadKey(roomId, thread) : roomKey(roomId),
        eventId,
        destination,
      );

      if (shown.current) router.back();
    } catch (reason) {
      if (shown.current) {
        setError(String(reason));
        setSending(false);
      }
    }
  };

  return (
    <>
      <Stack.Screen options={{ title: t('actions.forward') }} />
      <FlatList
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        testID="forward-message"
        data={(rooms ?? []).filter((r) => matches(r.name, search))}
        keyExtractor={(r) => r.room_id}
        ListHeaderComponent={
          <View style={{ padding: 16, gap: 8 }}>
            <TextInput
              testID="forward-search"
              placeholder={t('message.find_room')}
              placeholderTextColor={palette.textMuted}
              value={search}
              onChangeText={setSearch}
              style={{
                color: palette.text,
                backgroundColor: palette.surface,
                padding: 12,
                borderRadius: 8,
              }}
            />
            {error ? <EmptyState icon="alert" title={error} /> : null}
          </View>
        }
        ListEmptyComponent={<EmptyState icon="search" title={t('message.no_rooms')} />}
        renderItem={({ item }) => (
          <ListRow
            icon="forward"
            title={item.name}
            chevron={false}
            value={sending ? '…' : undefined}
            testID={`forward-room-${item.room_id}`}
            onPress={() => {
              void forward(item.room_id);
            }}
          />
        )}
      />
    </>
  );
}
