// A room's details (pushed from its header), as Slack's channel details: its picture, name,
// topic and facts; quick actions (notifications, invite, settings or rename, leave); and its
// people (with requests to join and those banned, for those who may act on them), threads and
// files, one at a time under a segmented control.

import type {
  Moderation,
  NotificationMode,
  RoomMember,
  SharedFile,
  ThreadInfo,
} from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import * as Sharing from 'expo-sharing';
import { useEffect, useState } from 'react';
import { Alert, FlatList, Platform, StyleSheet, Text, TextInput, View } from 'react-native';

import { t, tp } from '../../../i18n';
import { routes } from '../../../routes';
import {
  fileForSharing,
  leaveRoom,
  listAllMembers,
  listFiles,
  listThreads,
  moderate,
  setNotificationMode,
} from '../../../state/actions/people';
import { filterMembers, fileOpens, may } from '../../../state/people';
import { useAppStore, useStore } from '../../../state/react';
import {
  account as accountOf,
  room as roomOf,
  roomWidgetCount,
  shownFiles,
  shownThreads,
} from '../../../state/selectors';
import { font } from '../../../theme/palette';
import { askToLeave } from '../../../ui/askToLeave';
import { Avatar } from '../../../ui/Avatar';
import { Divider, ListRow, ListSection } from '../../../ui/components';
import { RequestRow } from '../../../ui/admin/RequestRow';
import { Icon } from '../../../ui/icons';
import { FileRow, ThreadRow } from '../../../ui/people/details';
import { historyLabel, notificationHint, notificationLabel } from '../../../ui/people/labels';
import { QuickAction } from '../../../ui/people/QuickAction';
import { MemberRow } from '../../../ui/people/rows';
import { Segments } from '../../../ui/people/Segments';
import { usePalette } from '../../../ui/theme';

type Segment = 'members' | 'threads' | 'files';

/** One row of whichever list the segment shows. */
type Row =
  | { kind: 'member'; key: string; member: RoomMember }
  | { kind: 'heading'; key: string; title: string }
  | { kind: 'knock'; key: string; member: RoomMember }
  | { kind: 'banned'; key: string; member: RoomMember }
  | { kind: 'thread'; key: string; thread: ThreadInfo }
  | { kind: 'file'; key: string; file: SharedFile };

const MODES: ReadonlyArray<NotificationMode | null> = [null, 'AllMessages', 'MentionsOnly', 'Mute'];

export default function RoomInfoScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const room = useStore((s) => roomOf(s, account, roomId));
  const members = useStore((s) => accountOf(s, account)?.members[roomId]);
  const widgetCount = useStore((s) => roomWidgetCount(s, account, roomId));
  // Threads people the user ignores started stay out of the list (ADR 0011).
  const threads = useStore((s) => shownThreads(s, account, roomId));
  // What people the user ignores shared stays out of the list (ADR 0011).
  const files = useStore((s) => shownFiles(s, account, roomId));

  const defaultMode = useStore(
    (s) => accountOf(s, account)?.summary.default_notifications ?? 'AllMessages',
  );

  const [segment, setSegment] = useState<Segment>('members');
  const [search, setSearch] = useState('');
  const [modes, setModes] = useState(false);
  const [sharing, setSharing] = useState<string | null>(null);

  // Sync changing who is in the room drops the list (a request approved, someone unbanned): it is
  // asked for again while the screen shows.
  const membersGone = members === undefined;
  useEffect(() => {
    if (membersGone) listAllMembers(store, account, roomId);
  }, [store, account, roomId, membersGone]);
  // Each time a list is shown, it is fresh.
  useEffect(() => {
    if (segment === 'threads') listThreads(store, account, roomId);

    if (segment === 'files') listFiles(store, account, roomId);
  }, [store, account, roomId, segment]);

  if (!room) return null;
  const direct = room.kind === 'Direct';

  const confirmLeave = () =>
    askToLeave(room, (forget) => {
      // Back to the room list at once: the room leaves it once the engine left it.
      router.dismissAll();
      leaveRoom(store, account, roomId, forget).catch(() => {});
    });

  const openFile = (file: SharedFile) => {
    if (fileOpens(file.attachment.kind) === 'preview') {
      router.push(routes.preview(account, roomId, file.event_id));

      return;
    }

    if (sharing !== null) return;
    setSharing(file.event_id);
    fileForSharing(store, account, file.attachment)
      .then((uri) =>
        uri ? Sharing.shareAsync(uri, { dialogTitle: file.attachment.name }) : undefined,
      )
      // Closing the share sheet without sharing is no failure; a failed fetch was told already.
      .catch(() => {})
      .finally(() => setSharing(null));
  };

  const act = (member: RoomMember, action: Moderation) => {
    const run = () => moderate(store, account, roomId, member.person.id, action).catch(() => {});

    if (action.type !== 'Deny') {
      run();

      return;
    }

    Alert.alert(
      t('mobile.admin.deny_title', { name: member.person.name }),
      t('mobile.admin.deny_body'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        { text: t('mobile.admin.deny'), style: 'destructive', onPress: run },
      ],
    );
  };

  /** Those apart from the members, under their heading, when the user may act on any of them. */
  const apart = (
    kind: 'knock' | 'banned',
    people: RoomMember[],
    title: string,
    actions: Moderation[],
  ): Row[] => {
    if (!members) return [];

    const shown = filterMembers(people, search).filter((m) =>
      actions.some((action) => may(members, action, m)),
    );

    if (shown.length === 0) return [];

    return [
      { kind: 'heading', key: kind, title },
      ...shown.map((member): Row => ({ kind, key: member.person.id, member })),
    ];
  };

  const memberRows = (): Row[] => {
    const people = filterMembers(members?.people ?? [], search).map((member): Row => ({
      kind: 'member',
      key: member.person.id,
      member,
    }));

    const requests = apart('knock', members?.knocking ?? [], t('mobile.admin.requests'), [
      { type: 'Admit' },
      { type: 'Deny' },
    ]);

    const banned = apart('banned', members?.banned ?? [], t('mobile.admin.banned'), [
      { type: 'Unban' },
    ]);

    if (requests.length === 0 && banned.length === 0) return people;

    return [
      ...requests,
      ...(people.length > 0
        ? [{ kind: 'heading', key: 'members', title: t('members.title') } as const]
        : []),
      ...people,
      ...banned,
    ];
  };

  const rows: Row[] =
    segment === 'members'
      ? memberRows()
      : segment === 'threads'
        ? (threads ?? []).map((thread) => ({ kind: 'thread', key: thread.root, thread }))
        : (files ?? []).map((file) => ({ kind: 'file', key: file.event_id, file }));

  const empty =
    segment === 'members'
      ? members === undefined
        ? t('members.loading')
        : t('mobile.people.no_members')
      : (segment === 'threads' ? threads : files) === undefined
        ? t('info.loading')
        : segment === 'threads'
          ? t('info.no_threads')
          : t('info.no_files');

  const header = (
    <View>
      <View style={styles.head}>
        <Avatar
          account={account}
          id={direct ? (room.dm_users[0] ?? room.room_id) : room.room_id}
          name={room.name}
          media={room.avatar}
          size={84}
        />
        <Text style={[font.title, styles.center, { color: palette.text }]} selectable>
          {room.name}
        </Text>
        <Text
          testID="info-topic"
          style={[font.body, styles.center, { color: palette.textMuted }]}
          selectable
        >
          {room.topic ?? t('room.no_topic')}
        </Text>
      </View>
      <View style={styles.actions}>
        <QuickAction
          testID="info-notifications"
          icon={room.notifications === 'Mute' ? 'bellOff' : 'bell'}
          label={notificationLabel(room.notifications)}
          active={modes}
          onPress={() => setModes((open) => !open)}
        />
        {members?.can_invite ? (
          <QuickAction
            testID="info-invite"
            icon="userAdd"
            label={t('mobile.people.action.invite')}
            onPress={() => router.push(routes.invite(account, roomId))}
          />
        ) : null}
        {room.can_change_settings ? (
          <QuickAction
            testID="info-settings"
            icon="settings"
            label={t('mobile.admin.settings')}
            onPress={() => router.push(routes.roomSettings(account, roomId))}
          />
        ) : null}
        <QuickAction
          testID="info-report"
          icon="flag"
          label={t('report.action')}
          onPress={() =>
            router.push(
              routes.report(account, { type: 'Room', data: { room_id: roomId } }, room.name),
            )
          }
        />
        <QuickAction
          testID="info-leave"
          icon={direct ? 'trash' : 'leave'}
          label={direct ? t('mobile.people.action.delete') : t('mobile.people.action.leave')}
          danger
          onPress={confirmLeave}
        />
      </View>
      {modes ? (
        <ListSection title={t('notify.heading')}>
          {MODES.map((mode, index) => (
            <View key={mode ?? 'default'}>
              {index > 0 ? <Divider inset={16} /> : null}
              <ListRow
                testID={`info-mode-${mode ?? 'default'}`}
                title={mode === null ? t('notify.default') : notificationLabel(mode)}
                subtitle={
                  mode === null
                    ? t('notify.default_hint', { mode: notificationLabel(defaultMode) })
                    : notificationHint(mode)
                }
                selected={
                  mode === null
                    ? !room.custom_notifications
                    : room.custom_notifications && room.notifications === mode
                }
                chevron={false}
                onPress={() => {
                  setModes(false);
                  setNotificationMode(store, account, roomId, mode);
                }}
              />
            </View>
          ))}
        </ListSection>
      ) : null}
      <ListSection>
        <ListRow
          icon="pin"
          title={t('info.pins')}
          value={String(room.pinned.length)}
          testID="info-pins"
          onPress={() => router.push(routes.pins(account, roomId))}
        />
        <Divider inset={52} />
        <ListRow
          icon="widgets"
          title={t('widgets.title')}
          value={widgetCount === undefined ? undefined : String(widgetCount)}
          testID="info-widgets"
          onPress={() => router.push(routes.widgets(account, roomId))}
        />
        <Divider inset={52} />
        <ListRow
          testID="info-members"
          icon="users"
          title={t('members.count', { count: room.member_count })}
          subtitle={room.knocks > 0 ? tp('mobile.admin.requests_count', room.knocks) : undefined}
        />
        <Divider inset={52} />
        <ListRow icon="eye" title={historyLabel(room.history)} />
      </ListSection>
      <View style={styles.segments}>
        <Segments<Segment>
          testID="info-segments"
          selected={segment}
          onChange={setSegment}
          options={[
            { value: 'members', label: t('members.title') },
            { value: 'threads', label: t('info.threads') },
            { value: 'files', label: t('info.files') },
          ]}
        />
      </View>
      {segment === 'members' ? (
        <View
          style={[
            styles.search,
            { backgroundColor: palette.surface, borderColor: palette.divider },
          ]}
        >
          <Icon name="search" size={16} color={palette.textMuted} />
          <TextInput
            testID="member-search"
            value={search}
            onChangeText={setSearch}
            placeholder={t('members.search')}
            placeholderTextColor={palette.textMuted}
            selectionColor={palette.accent}
            autoCapitalize="none"
            autoCorrect={false}
            clearButtonMode="while-editing"
            returnKeyType="search"
            style={[font.body, styles.searchInput, { color: palette.text }]}
          />
        </View>
      ) : null}
    </View>
  );

  return (
    <>
      <Stack.Screen options={{ title: t('info.title') }} />
      <FlatList<Row>
        testID="room-info"
        data={rows}
        keyExtractor={(row) => `${row.kind}-${row.key}`}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode="on-drag"
        ListHeaderComponent={header}
        ItemSeparatorComponent={({ leadingItem }: { leadingItem?: Row }) =>
          leadingItem?.kind === 'heading' ? null : (
            <Divider inset={segment === 'threads' ? 16 : 68} />
          )
        }
        ListEmptyComponent={
          <Text style={[font.body, styles.empty, { color: palette.textMuted }]}>{empty}</Text>
        }
        contentContainerStyle={styles.list}
        renderItem={({ item }) => {
          switch (item.kind) {
            case 'heading':
              return (
                <Text
                  style={[
                    font.section,
                    styles.heading,
                    { color: Platform.OS === 'android' ? palette.accent : palette.textMuted },
                  ]}
                >
                  {Platform.OS === 'android' ? item.title : item.title.toUpperCase()}
                </Text>
              );
            case 'knock':
              return (
                <RequestRow
                  account={account}
                  member={item.member}
                  actions={[
                    ...(members && may(members, { type: 'Deny' }, item.member)
                      ? [
                          {
                            title: t('mobile.admin.deny'),
                            kind: 'secondary' as const,
                            testID: `deny-${item.member.person.id}`,
                            onPress: () => act(item.member, { type: 'Deny' }),
                          },
                        ]
                      : []),
                    ...(members && may(members, { type: 'Admit' }, item.member)
                      ? [
                          {
                            title: t('mobile.admin.approve'),
                            kind: 'primary' as const,
                            testID: `approve-${item.member.person.id}`,
                            onPress: () => act(item.member, { type: 'Admit' }),
                          },
                        ]
                      : []),
                  ]}
                />
              );
            case 'banned':
              return (
                <RequestRow
                  account={account}
                  member={item.member}
                  actions={[
                    {
                      title: t('mobile.admin.unban'),
                      kind: 'secondary',
                      testID: `unban-${item.member.person.id}`,
                      onPress: () => act(item.member, { type: 'Unban' }),
                    },
                  ]}
                />
              );
            case 'member':
              return (
                <MemberRow
                  account={account}
                  member={item.member}
                  me={item.member.person.id === account}
                  onPress={() => router.push(routes.member(account, roomId, item.member.person.id))}
                />
              );
            case 'thread':
              return (
                <ThreadRow
                  thread={item.thread}
                  onPress={() => router.push(routes.thread(account, roomId, item.thread.root))}
                />
              );
            case 'file':
              return (
                <FileRow
                  file={item.file}
                  busy={sharing === item.file.event_id}
                  onPress={() => openFile(item.file)}
                />
              );
          }
        }}
      />
    </>
  );
}

const styles = StyleSheet.create({
  list: { paddingBottom: 32 },
  head: { alignItems: 'center', gap: 8, paddingHorizontal: 24, paddingTop: 20 },
  center: { textAlign: 'center' },
  actions: {
    flexDirection: 'row',
    justifyContent: 'center',
    gap: 8,
    marginTop: 20,
    paddingHorizontal: 12,
  },
  segments: { marginHorizontal: 16, marginTop: 22, marginBottom: 10 },
  search: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginHorizontal: 16,
    marginBottom: 6,
    paddingHorizontal: 12,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
  },
  searchInput: { flex: 1, paddingVertical: Platform.OS === 'ios' ? 10 : 6 },
  empty: { padding: 24, textAlign: 'center' },
  heading: { marginTop: 14, marginBottom: 4, marginHorizontal: 18 },
});
