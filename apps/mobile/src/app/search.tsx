// "Jump to…": the rooms, direct chats and spaces of the account on screen by name (the latest
// conversations before anything is typed), and starting a chat with someone new.

import type { RoomSummary } from '@better-matrix/matrix-native/types';
import { type Href, router, Stack } from 'expo-router';
import { useMemo, useState } from 'react';
import { Platform, SectionList, StyleSheet, Text } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { setScope } from '../state/actions/core';
import { jumpTo } from '../state/lists';
import { useAppStore, useStore } from '../state/react';
import { lastMessage } from '../state/sections';
import { activeAccount } from '../state/selectors';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { EmptyState, ListRow } from '../ui/components';
import { RoomRow } from '../ui/shell/RoomRow';
import { useNow } from '../ui/shell/useNow';
import { usePalette } from '../ui/theme';

type Item =
  | { kind: 'room'; room: RoomSummary }
  | { kind: 'space'; id: string; name: string; avatar: string | null }
  | { kind: 'chat' };

/** Closes the search, then opens `href` from where it was opened. */
function leaveFor(href: Href): void {
  router.back();
  router.push(href);
}

export default function SearchScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useStore(activeAccount);
  const [term, setTerm] = useState('');
  const now = useNow();

  const results = useMemo(
    () => (account ? jumpTo(account, term, now) : null),
    [account, term, now],
  );

  const typed = term.trim().length > 0;

  const sections = useMemo(() => {
    if (!results) return [];
    const out: Array<{ key: string; title: string | null; data: Item[] }> = [];

    if (typed) {
      out.push({
        key: 'rooms',
        title: t('search.rooms'),
        data: results.rooms.map((room) => ({ kind: 'room', room })),
      });
      out.push({
        key: 'direct',
        title: t('search.direct'),
        data: results.direct.map((room) => ({ kind: 'room', room })),
      });
      out.push({
        key: 'spaces',
        title: t('search.spaces'),
        data: results.spaces.map((s) => ({
          kind: 'space',
          id: s.room_id,
          name: s.name,
          avatar: s.avatar,
        })),
      });
    } else {
      const recent = [...results.rooms, ...results.direct];
      out.push({
        key: 'recent',
        title: t('overview.recent'),
        data: recent.map((room) => ({ kind: 'room', room })),
      });
    }

    out.push({ key: 'chat', title: null, data: [{ kind: 'chat' }] });

    return out.filter((s) => s.data.length > 0);
  }, [results, typed]);

  if (!account) return null;
  const id = account.summary.id;
  const found = !typed || sections.some((s) => s.key !== 'chat');

  return (
    <>
      <Stack.Screen
        options={{
          title: t('mobile.shell.jump_title'),
          headerShown: true,
          // A modal on iOS closes by a swipe or this; Android's back closes it.
          headerRight:
            Platform.OS === 'ios'
              ? () => (
                  <Text
                    style={[font.body, { color: palette.accent }]}
                    onPress={() => router.back()}
                  >
                    {t('common.close')}
                  </Text>
                )
              : undefined,
        }}
      />
      <Stack.SearchBar
        autoFocus
        hideWhenScrolling={false}
        obscureBackground={false}
        placeholder={t('mobile.shell.jump_to')}
        onChangeText={(e) => setTerm(e.nativeEvent.text)}
        textColor={palette.text}
        tintColor={palette.accent}
      />
      <SectionList
        testID="search-results"
        style={{ flex: 1, backgroundColor: palette.bg }}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode="on-drag"
        sections={sections}
        stickySectionHeadersEnabled={false}
        keyExtractor={(item, index) =>
          item.kind === 'room'
            ? item.room.room_id
            : item.kind === 'space'
              ? item.id
              : `chat-${index}`
        }
        ListHeaderComponent={
          found ? undefined : <EmptyState icon="search" title={t('search.nothing')} />
        }
        renderSectionHeader={({ section }) =>
          section.title ? (
            <Text style={[font.section, styles.title, { color: palette.textMuted }]}>
              {section.title.toUpperCase()}
            </Text>
          ) : null
        }
        renderItem={({ item }) => {
          if (item.kind === 'room') {
            return (
              <RoomRow
                account={id}
                room={item.room}
                last={lastMessage(account, item.room.room_id)}
                now={now}
                onPress={() => leaveFor(routes.room(id, item.room.room_id))}
              />
            );
          }

          if (item.kind === 'space') {
            return (
              <ListRow
                testID={`search-space-${item.id}`}
                title={item.name}
                leading={
                  <Avatar
                    account={id}
                    id={item.id}
                    name={item.name}
                    media={item.avatar}
                    size={40}
                  />
                }
                onPress={() => {
                  setScope(store, id, { space: item.id });
                  router.back();
                  router.navigate(routes.home());
                }}
              />
            );
          }

          return (
            <ListRow
              testID="search-new-chat"
              icon="compose"
              title={t('create.chat')}
              onPress={() => leaveFor(routes.newChat())}
            />
          );
        }}
      />
    </>
  );
}

const styles = StyleSheet.create({
  title: { paddingHorizontal: 18, paddingTop: 18, paddingBottom: 6 },
});
