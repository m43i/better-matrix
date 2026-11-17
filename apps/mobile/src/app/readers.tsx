// Who has read up to a message (sheet, from its read indicator), newest first, with when. A
// reader who is a member opens the member sheet, as the desktop's list opens member actions.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect } from 'react';
import { FlatList, StyleSheet, Text } from 'react-native';

import { tp } from '../i18n';
import { routes } from '../routes';
import { listAllMembers } from '../state/actions/people';
import { memberOf } from '../state/people';
import { readerOf, readersOf } from '../state/readers';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf } from '../state/selectors';
import { sentAt } from '../state/timelineText';
import { font } from '../theme/palette';
import { Divider } from '../ui/components';
import { useNow } from '../ui/shell/useNow';
import { PersonRow } from '../ui/people/rows';
import { usePalette } from '../ui/theme';

export default function ReadersScreen() {
  const { account, roomId, eventId } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const readers = useStore((s) => readersOf(s, account, roomId, eventId));
  const data = useStore((s) => accountOf(s, account));

  // Names, pictures and member actions come from the room's whole member list.
  useEffect(() => listAllMembers(store, account, roomId), [store, account, roomId]);

  const now = useNow();

  return (
    <FlatList
      testID="readers"
      data={readers}
      keyExtractor={(receipt) => receipt.user_id}
      contentContainerStyle={{ paddingBottom: 24 }}
      ItemSeparatorComponent={() => <Divider inset={68} />}
      ListHeaderComponent={
        <Text style={[font.header, styles.title, { color: palette.text }]}>
          {tp('message.seen_by', readers.length)}
        </Text>
      }
      renderItem={({ item }) => {
        const person = readerOf(data, roomId, item.user_id);
        const member = memberOf(data?.members[roomId], item.user_id);

        return (
          <PersonRow
            account={account}
            person={person}
            testID={`reader-${item.user_id}`}
            trailing={
              item.timestamp_ms === null ? null : (
                <Text style={[font.meta, { color: palette.textMuted }]}>
                  {sentAt(item.timestamp_ms, now)}
                </Text>
              )
            }
            onPress={
              member ? () => router.push(routes.member(account, roomId, item.user_id)) : undefined
            }
          />
        );
      }}
    />
  );
}

const styles = StyleSheet.create({
  title: { paddingHorizontal: 16, paddingTop: 20, paddingBottom: 8 },
});
