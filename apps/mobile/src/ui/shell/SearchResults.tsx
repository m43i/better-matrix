// What the search under a tab's title finds, in the tab's place while something is typed, as the
// desktop's top bar lists it: a typed link or address to open (a pasted invitation,
// `#room:server`), the account's rooms, direct chats and spaces by name, then people from the
// user directory. Picking one empties the search (`onPicked`).

import type { Person } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { useEffect, useMemo } from 'react';
import { SectionList, StyleSheet, Text } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { setScope } from '../../state/actions/core';
import { openDirectChat } from '../../state/actions/people';
import { foundAny, searchResults } from '../../state/lists';
import { busy } from '../../state/people';
import { type Permalink, permalinkUri } from '../../state/permalinks';
import { useAppStore } from '../../state/react';
import { lastMessage } from '../../state/sections';
import type { AccountData } from '../../state/types';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { EmptyState, ListRow } from '../components';
import { openPermalink } from '../links/open';
import { usePeopleSearch } from '../people/usePeopleSearch';
import { usePalette } from '../theme';
import { RoomRow } from './RoomRow';
import { useNow } from './useNow';

type Item =
  | { kind: 'link'; link: Permalink }
  | { kind: 'room'; room: AccountData['rooms'][number] }
  | { kind: 'space'; space: AccountData['spaces'][number] }
  | { kind: 'person'; person: Person };

interface Section {
  key: string;
  title: string;
  data: Item[];
}

export function SearchResults({
  account,
  term,
  onPicked,
}: {
  account: AccountData;
  term: string;
  /** A result was picked: the search empties. */
  onPicked: () => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const now = useNow();
  const id = account.summary.id;
  // Only what is typed is searched: the tab's list shows while nothing is.
  const [people, type] = usePeopleSearch(id, false);
  useEffect(() => type(term), [term, type]);

  const results = useMemo(
    () => searchResults(account, term, people.people, now),
    [account, term, people.people, now],
  );

  const sections = useMemo(() => {
    const all: Section[] = [
      {
        key: 'link',
        title: t('search.link'),
        data: results.link ? [{ kind: 'link', link: results.link }] : [],
      },
      {
        key: 'rooms',
        title: t('search.rooms'),
        data: results.rooms.map((room) => ({ kind: 'room', room })),
      },
      {
        key: 'direct',
        title: t('search.direct'),
        data: results.direct.map((room) => ({ kind: 'room', room })),
      },
      {
        key: 'spaces',
        title: t('search.spaces'),
        data: results.spaces.map((space) => ({ kind: 'space', space })),
      },
      {
        key: 'people',
        title: t('search.people'),
        data: results.people.map((person) => ({ kind: 'person', person })),
      },
    ];

    return all.filter((section) => section.data.length > 0);
  }, [results]);

  const chat = (userId: string) => {
    onPicked();
    openDirectChat(store, id, userId).then(
      (roomId) => router.push(routes.room(id, roomId)),
      // The notice says why.
      () => undefined,
    );
  };

  const render = (item: Item) => {
    switch (item.kind) {
      case 'link':
        return (
          <ListRow
            testID="search-link"
            icon="link"
            title={t('search.open_link', { target: item.link.id })}
            onPress={() => {
              onPicked();
              openPermalink(store, permalinkUri(item.link), id);
            }}
          />
        );
      case 'room':
        return (
          <RoomRow
            account={id}
            room={item.room}
            last={lastMessage(account, item.room.room_id)}
            now={now}
            onPress={() => {
              onPicked();
              router.push(routes.room(id, item.room.room_id));
            }}
          />
        );
      case 'space':
        return (
          <ListRow
            testID={`search-space-${item.space.room_id}`}
            title={item.space.name}
            leading={
              <Avatar
                account={id}
                id={item.space.room_id}
                name={item.space.name}
                media={item.space.avatar}
                size={32}
              />
            }
            onPress={() => {
              onPicked();
              setScope(store, id, { space: item.space.room_id });
              router.navigate(routes.home());
            }}
          />
        );
      case 'person':
        return (
          <ListRow
            testID={`search-person-${item.person.id}`}
            title={item.person.name || item.person.id}
            subtitle={item.person.id}
            leading={
              <Avatar
                account={id}
                id={item.person.id}
                name={item.person.name || item.person.id}
                media={item.person.avatar}
                size={32}
              />
            }
            onPress={() => chat(item.person.id)}
          />
        );
    }
  };

  return (
    <SectionList
      testID="search-results"
      style={styles.fill}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      keyboardDismissMode="on-drag"
      sections={sections}
      stickySectionHeadersEnabled={false}
      keyExtractor={(item) => {
        switch (item.kind) {
          case 'link':
            return `link-${item.link.id}`;
          case 'room':
            return item.room.room_id;
          case 'space':
            return item.space.room_id;
          case 'person':
            return `person-${item.person.id}`;
        }
      }}
      ListEmptyComponent={
        foundAny(results) ? undefined : (
          <EmptyState
            loading={busy(people)}
            title={busy(people) ? t('search.searching') : t('search.nothing')}
          />
        )
      }
      renderSectionHeader={({ section }) => (
        <Text style={[font.section, styles.title, { color: palette.textMuted }]}>
          {section.title.toUpperCase()}
        </Text>
      )}
      renderItem={({ item }) => render(item)}
    />
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  title: { paddingHorizontal: 18, paddingTop: 18, paddingBottom: 6 },
});
