// What the search under a tab's title finds, in the tab's place while something is typed, as the
// desktop's top bar lists it: a typed link or address to open (a pasted invitation,
// `#room:server`), the account's rooms, direct chats and spaces by name, then people from the
// user directory. Picking one empties the search (`onPicked`).

import type { MessageHit, Person } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { useEffect, useMemo } from 'react';
import { SectionList, StyleSheet, Text } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { setScope } from '../../state/actions/core';
import { openDirectChat } from '../../state/actions/people';
import { foundAny, listTime, searchResults } from '../../state/lists';
import { busy } from '../../state/people';
import { type Permalink, permalinkUri } from '../../state/permalinks';
import { shallow, useAppStore, useStore } from '../../state/react';
import { lastMessage } from '../../state/sections';
import { roomListOf } from '../../state/selectors';
import type { AccountData } from '../../state/types';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { EmptyState, ListRow } from '../components';
import { openPermalink } from '../links/open';
import { usePeopleSearch } from '../people/usePeopleSearch';
import { usePalette } from '../theme';
import { useShown } from '../useShown';
import { RoomRow } from './RoomRow';
import { SearchHighlight } from './SearchHighlight';
import { useMessageSearch } from './useMessageSearch';
import { useNow } from './useNow';

type Item =
  | { kind: 'link'; link: Permalink }
  | { kind: 'room'; room: AccountData['rooms'][number] }
  | { kind: 'space'; space: AccountData['spaces'][number] }
  | { kind: 'person'; person: Person }
  | { kind: 'message'; hit: MessageHit };

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
  const shown = useShown();
  const palette = usePalette();
  const now = useNow();
  const id = account.summary.id;
  // Rooms preview their newest message as the room lists do: without a pending ignore's person.
  const list = useStore((s) => roomListOf(s, id), shallow);
  // Only what is typed is searched: the tab's list shows while nothing is.
  const [people, type] = usePeopleSearch(id, false);
  const messages = useMessageSearch(id, term);
  useEffect(() => type(term), [term, type]);

  const results = useMemo(
    () => searchResults(account, term, people.people, now, messages.messages),
    [account, term, people.people, now, messages.messages],
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
      {
        key: 'messages',
        title: t('search.messages'),
        data: results.messages.map((hit) => ({ kind: 'message', hit })),
      },
    ];

    return all.filter((section) => section.data.length > 0);
  }, [results]);

  const loading = busy(people) || messages.status === 'waiting' || messages.status === 'searching';
  const any = foundAny(results);

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
              openPermalink(store, permalinkUri(item.link), id, () => shown.current);
            }}
          />
        );
      case 'room':
        return (
          <RoomRow
            account={id}
            room={item.room}
            query={term}
            last={list && lastMessage(list, item.room.room_id)}
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
            titleContent={<SearchHighlight text={item.space.name} query={term} />}
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
      case 'message': {
        const hit = item.hit;
        const room = account.rooms.find((r) => r.room_id === hit.room_id);

        const sender =
          account.members[hit.room_id]?.people.find((m) => m.person.id === hit.sender)?.person
            .name ?? hit.sender;

        return (
          <ListRow
            testID={`search-message-${hit.event_id}`}
            title={hit.body}
            titleContent={<SearchHighlight text={hit.body} positions={hit.highlights} />}
            subtitle={`${room?.name ?? hit.room_id} · ${sender} · ${listTime(hit.timestamp_ms, now)}`}
            onPress={() => {
              onPicked();

              if (hit.thread)
                router.push(routes.thread(id, hit.room_id, hit.thread, hit.event_id, 'local'));
              else router.push(routes.room(id, hit.room_id, hit.event_id, 'local'));
            }}
          />
        );
      }

      case 'person':
        return (
          <ListRow
            testID={`search-person-${item.person.id}`}
            title={item.person.name || item.person.id}
            titleContent={
              <SearchHighlight text={item.person.name || item.person.id} query={term} />
            }
            subtitle={item.person.id}
            subtitleContent={<SearchHighlight text={item.person.id} query={term} />}
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
      contentContainerStyle={styles.content}
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
          case 'message':
            return `message-${item.hit.room_id}-${item.hit.event_id}`;
          case 'person':
            return `person-${item.person.id}`;
        }
      }}
      ListFooterComponent={
        any &&
        (messages.status === 'failed' ||
          messages.status === 'searching' ||
          messages.status === 'waiting') ? (
          <Text style={[font.meta, styles.title, { color: palette.textMuted }]}>
            {messages.status === 'failed' ? t('search.messages_failed') : t('search.searching')}
          </Text>
        ) : undefined
      }
      ListEmptyComponent={
        any ? undefined : (
          <EmptyState
            loading={loading}
            title={
              loading
                ? t('search.searching')
                : messages.status === 'failed'
                  ? t('search.messages_failed')
                  : t('search.nothing')
            }
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
  content: { paddingBottom: 96 },
  title: { paddingHorizontal: 18, paddingTop: 18, paddingBottom: 6 },
});
