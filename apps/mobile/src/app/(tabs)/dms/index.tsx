// Direct messages: every direct chat of the account on screen, latest first, whichever space
// the Home tab shows. The header holds the account's picture and the search, as Home's does;
// Android's floating button opens the create menu.

import { router } from 'expo-router';
import { useMemo } from 'react';
import { FlatList, StyleSheet, View } from 'react-native';

import { t } from '../../../i18n';
import { routes } from '../../../routes';
import { useStore } from '../../../state/react';
import { directChats, lastMessage } from '../../../state/sections';
import { activeAccount } from '../../../state/selectors';
import { Button, EmptyState } from '../../../ui/components';
import { CallBar } from '../../../ui/call/CallBar';
import { CreateFab } from '../../../ui/shell/CreateFab';
import { RoomRow } from '../../../ui/shell/RoomRow';
import { TabHeader } from '../../../ui/shell/TabHeader';
import { useNow } from '../../../ui/shell/useNow';
import { useShellSearch } from '../../../ui/shell/useShellSearch';

export default function DirectMessagesScreen() {
  const account = useStore(activeAccount);
  const search = useShellSearch(account);
  const now = useNow();
  const chats = useMemo(() => (account ? directChats(account, now) : []), [account, now]);

  if (!account) return null;
  const id = account.summary.id;

  return (
    <>
      <TabHeader title={t('sidebar.direct')} />
      {search.bar}
      {search.results ??
        (!account.roomsLoaded ? (
          <EmptyState loading title={t('sidebar.syncing')} />
        ) : (
          <FlatList
            testID="direct-chats"
            style={styles.fill}
            contentInsetAdjustmentBehavior="automatic"
            data={chats}
            keyExtractor={(room) => room.room_id}
            ListHeaderComponent={<CallBar />}
            renderItem={({ item }) => (
              <RoomRow
                account={id}
                room={item}
                last={lastMessage(account, item.room_id)}
                now={now}
                detailed
              />
            )}
            ListEmptyComponent={
              <View style={styles.empty}>
                <EmptyState
                  icon="chat"
                  title={t('sidebar.empty')}
                  body={t('mobile.shell.dms_empty')}
                />
                <Button
                  title={t('create.chat')}
                  icon="compose"
                  onPress={() => router.push(routes.newChat())}
                  testID="dms-new-chat"
                />
              </View>
            }
          />
        ))}
      <CreateFab />
    </>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  empty: { paddingHorizontal: 32, paddingTop: 48, gap: 16 },
});
