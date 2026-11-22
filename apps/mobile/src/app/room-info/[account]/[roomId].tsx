// A room's details (pushed from its header), as Slack's channel details: its picture, name,
// topic and facts; quick actions (notifications, invite, rename, leave); and its people, threads
// and files, one at a time under a segmented control.

import type {
  NotificationMode,
  RoomMember,
  SharedFile,
  ThreadInfo,
} from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import * as Sharing from 'expo-sharing';
import { useEffect, useState } from 'react';
import { Alert, FlatList, Platform, StyleSheet, Text, TextInput, View } from 'react-native';

import { t } from '../../../i18n';
import { routes } from '../../../routes';
import {
  fileForSharing,
  leaveRoom,
  listAllMembers,
  listFiles,
  listThreads,
  setNotificationMode,
} from '../../../state/actions/people';
import { filterMembers, fileOpens, mayRename } from '../../../state/people';
import { useAppStore, useStore } from '../../../state/react';
import { account as accountOf, room as roomOf, shownFiles } from '../../../state/selectors';
import { font } from '../../../theme/palette';
import { Avatar } from '../../../ui/Avatar';
import { Divider, ListRow, ListSection } from '../../../ui/components';
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
  const threads = useStore((s) => accountOf(s, account)?.threads[roomId]);
  // What people the user ignores shared stays out of the list (ADR 0011).
  const files = useStore((s) => shownFiles(s, account, roomId));

  const defaultMode = useStore(
    (s) => accountOf(s, account)?.summary.default_notifications ?? 'AllMessages',
  );

  const [segment, setSegment] = useState<Segment>('members');
  const [search, setSearch] = useState('');
  const [modes, setModes] = useState(false);
  const [sharing, setSharing] = useState<string | null>(null);

  useEffect(() => listAllMembers(store, account, roomId), [store, account, roomId]);
  // Each time a list is shown, it is fresh.
  useEffect(() => {
    if (segment === 'threads') listThreads(store, account, roomId);

    if (segment === 'files') listFiles(store, account, roomId);
  }, [store, account, roomId, segment]);

  if (!room) return null;
  const direct = room.kind === 'Direct';

  const askToLeave = () => {
    const [title, body, confirm] = direct
      ? [
          t('confirm.delete_title', { name: room.name }),
          t('confirm.delete_body', { name: room.name }),
          t('confirm.delete'),
        ]
      : [
          t('confirm.leave_title', { room: room.name }),
          t('confirm.leave_body'),
          t('confirm.leave'),
        ];

    Alert.alert(title, body, [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: confirm,
        style: 'destructive',
        onPress: () => {
          // Back to the room list at once: the room leaves it once the engine left it.
          router.dismissAll();
          leaveRoom(store, account, roomId, direct).catch(() => {});
        },
      },
    ]);
  };

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

  const rows: Row[] =
    segment === 'members'
      ? filterMembers(members?.people ?? [], search).map((member) => ({
          kind: 'member',
          key: member.person.id,
          member,
        }))
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
        {mayRename(room, members) ? (
          <QuickAction
            testID="info-rename"
            icon="pencil"
            label={t('create.rename_title')}
            onPress={() => router.push(routes.renameRoom(account, roomId))}
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
          onPress={askToLeave}
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
        <ListRow icon="users" title={t('members.count', { count: room.member_count })} />
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
        ItemSeparatorComponent={() => <Divider inset={segment === 'threads' ? 16 : 68} />}
        ListEmptyComponent={
          <Text style={[font.body, styles.empty, { color: palette.textMuted }]}>{empty}</Text>
        }
        contentContainerStyle={styles.list}
        renderItem={({ item }) => {
          switch (item.kind) {
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
});
