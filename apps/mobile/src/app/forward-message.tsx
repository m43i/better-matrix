// Choose a joined conversation on the message's account; failures leave the picker open.
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { SectionList, Text, TextInput, View } from 'react-native';
import { t } from '../i18n';
import { forwardMessage } from '../state/actions/message-actions';
import { forwardSections } from '../state/messages';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Icon } from '../ui/icons';
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
      <SectionList
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        testID="forward-message"
        sections={forwardSections(rooms ?? [], search)}
        stickySectionHeadersEnabled={false}
        renderSectionHeader={({ section }) => (
          <Text
            testID={`forward-section-${section.kind}`}
            style={[
              font.section,
              {
                color: palette.textMuted,
                backgroundColor: palette.bg,
                paddingHorizontal: 16,
                paddingTop: 16,
                paddingBottom: 6,
              },
            ]}
          >
            {t(section.kind === 'Group' ? 'sidebar.rooms' : 'sidebar.direct')}
          </Text>
        )}
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
            leading={
              <View style={{ width: 28, alignItems: 'center' }}>
                {item.kind === 'Group' ? (
                  <Icon name="hash" size={22} color={palette.textMuted} />
                ) : (
                  <Avatar
                    account={account}
                    id={item.dm_users[0] ?? item.room_id}
                    name={item.name}
                    media={item.avatar}
                    size={28}
                  />
                )}
              </View>
            }
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
