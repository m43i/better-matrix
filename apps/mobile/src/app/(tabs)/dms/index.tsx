// Direct messages: every direct chat of the account on screen, latest first, whichever space
// the Home tab shows. The header starts a new chat.

import { router, Stack } from 'expo-router';
import { useMemo } from 'react';
import { FlatList, StyleSheet, View } from 'react-native';

import { t } from '../../../i18n';
import { routes } from '../../../routes';
import { useStore } from '../../../state/react';
import { directChats, lastMessage } from '../../../state/sections';
import { activeAccount } from '../../../state/selectors';
import { Button, EmptyState } from '../../../ui/components';
import { CallBar } from '../../../ui/call/CallBar';
import { RoomRow } from '../../../ui/shell/RoomRow';
import { useNow } from '../../../ui/shell/useNow';
import { useToolbarIcon } from '../../../ui/shell/toolbarIcon';
import { usePalette } from '../../../ui/theme';

export default function DirectMessagesScreen() {
  const palette = usePalette();
  const account = useStore(activeAccount);
  const compose = useToolbarIcon('compose', palette.text);
  const now = useNow();
  const chats = useMemo(() => (account ? directChats(account, now) : []), [account, now]);

  if (!account) return null;
  const id = account.summary.id;

  return (
    <>
      <Stack.Screen options={{ title: t('sidebar.direct') }} />
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon={compose}
          accessibilityLabel={t('create.chat')}
          onPress={() => router.push(routes.newChat())}
        />
      </Stack.Toolbar>
      {!account.roomsLoaded ? (
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
      )}
    </>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  empty: { paddingHorizontal: 32, paddingTop: 48, gap: 16 },
});
