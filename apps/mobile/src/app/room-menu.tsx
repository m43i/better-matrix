// The sheet behind a long press in the room list (the desktop's room menu and section menu).
// For a room: its notifications, its place among the user's sections (the phone's form of the
// desktop's drag and drop), marking it read, and leaving it or deleting the chat. For a section:
// collapsing it, and renaming or deleting the user's own. Without either, naming a new section.
// Its pages are an explicit state machine (`Page`); edits apply to the account's current scope.

import type { AccountId, NotificationMode, RoomSummary } from '@better-matrix/matrix-native/types';
import { router, useLocalSearchParams } from 'expo-router';
import { useState } from 'react';
import { Alert, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import {
  addSection,
  deleteSection,
  leaveRoom,
  markRoomRead,
  moveRoomToSection,
  renameSection,
  setNotificationMode,
  toggleSection,
} from '../state/actions/shell';
import { useAppStore, useStore } from '../state/react';
import { DIRECT, isBuiltin, ROOMS, roomsInScope, scopeKey, sectionOf } from '../state/sections';
import { account as accountOf } from '../state/selectors';
import type { AccountData, Scope } from '../state/types';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Button, Divider, EmptyState, ListRow, ListSection, TextField } from '../ui/components';
import type { IconName } from '../ui/icons';
import { usePalette } from '../ui/theme';

type Page =
  | { kind: 'menu' }
  | { kind: 'notifications' }
  | { kind: 'move' }
  /** Naming a section: a new one (`section: null`, then holding `roomId` if given) or a rename. */
  | { kind: 'name'; section: string | null; roomId: string | null };

function param(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}

export default function RoomMenuScreen() {
  const params = useLocalSearchParams<{ account: string; roomId?: string; section?: string }>();
  const accountId = param(params.account) ?? '';
  const roomId = param(params.roomId);
  const section = param(params.section);
  const account = useStore((s) => accountOf(s, accountId));
  const palette = usePalette();
  const [page, setPage] = useState<Page>(roomId || section ? { kind: 'menu' } : { kind: 'name', section: null, roomId: null });

  let body;
  if (!account) {
    body = <EmptyState title={t('sidebar.empty')} />;
  } else if (page.kind === 'name') {
    body = <SectionName account={account} page={page} />;
  } else if (roomId) {
    const room = account.rooms.find((r) => r.room_id === roomId);
    body = room ? <RoomPages account={account} room={room} page={page} setPage={setPage} /> : <EmptyState title={t('sidebar.empty')} />;
  } else if (section) {
    body = <SectionPage account={account} id={section} setPage={setPage} />;
  }
  return (
    // A plain view: the sheet fits its content (`fitToContents`), which a scroll view hides.
    <View style={[styles.content, { backgroundColor: palette.bg }]}>{body}</View>
  );
}

/** The modes a room can notify in, as the desktop's room menu lists them. */
const MODES: Array<{ mode: NotificationMode; id: string; title: () => string; hint: () => string; icon: IconName }> = [
  { mode: 'AllMessages', id: 'all', title: () => t('notify.all'), hint: () => t('notify.all_hint'), icon: 'bell' },
  { mode: 'MentionsOnly', id: 'mentions', title: () => t('notify.mentions'), hint: () => t('notify.mentions_hint'), icon: 'at' },
  { mode: 'Mute', id: 'mute', title: () => t('notify.mute'), hint: () => t('notify.mute_hint'), icon: 'bellOff' },
];

function modeLabel(mode: NotificationMode): string {
  return MODES.find((m) => m.mode === mode)?.title() ?? '';
}

function RoomPages({
  account,
  room,
  page,
  setPage,
}: {
  account: AccountData;
  room: RoomSummary;
  page: Page;
  setPage: (page: Page) => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const id = account.summary.id;
  const scope = account.scope;
  const direct = room.kind === 'Direct';
  const inScope = roomsInScope(account, scope).some((r) => r.room_id === room.room_id);
  const current = sectionOf(account.sidebar, scope, room.room_id);
  const own = account.sidebar.scopes[scopeKey(scope)]?.sections ?? [];
  const done = () => router.back();

  const header = (
    <View style={styles.header}>
      <Avatar account={id} id={room.dm_users[0] ?? room.room_id} name={room.name} media={room.avatar} size={36} />
      <Text style={[font.header, styles.grow, { color: palette.text }]} numberOfLines={1}>
        {room.name}
      </Text>
    </View>
  );

  if (page.kind === 'notifications') {
    return (
      <>
        {header}
        <ListSection title={t('notify.heading')}>
          {MODES.map(({ mode, id: modeId, title, hint, icon }) => (
            <View key={modeId}>
              <ListRow
                testID={`notify-${modeId}`}
                icon={icon}
                title={title()}
                subtitle={hint()}
                selected={room.custom_notifications && room.notifications === mode}
                chevron={false}
                onPress={() => {
                  setNotificationMode(store, id, room.room_id, mode);
                  done();
                }}
              />
              <Divider inset={52} />
            </View>
          ))}
          <ListRow
            testID="notify-default"
            icon="settings"
            title={t('notify.default')}
            subtitle={t('notify.default_hint', { mode: modeLabel(account.summary.default_notifications) })}
            selected={!room.custom_notifications}
            chevron={false}
            onPress={() => {
              setNotificationMode(store, id, room.room_id, null);
              done();
            }}
          />
        </ListSection>
      </>
    );
  }

  if (page.kind === 'move') {
    return (
      <>
        {header}
        <ListSection title={t('mobile.shell.move_to_section')}>
          {own.map((s) => (
            <View key={s.id}>
              <ListRow
                testID={`move-${s.id}`}
                icon="folder"
                title={s.name}
                selected={s.id === current}
                chevron={false}
                onPress={() => {
                  moveRoomToSection(store, id, scope, room.room_id, s.id);
                  done();
                }}
              />
              <Divider inset={52} />
            </View>
          ))}
          <ListRow
            testID="move-new-section"
            icon="folderAdd"
            title={t('sections.new')}
            onPress={() => setPage({ kind: 'name', section: null, roomId: room.room_id })}
          />
        </ListSection>
      </>
    );
  }

  const confirmLeave = () => {
    const [title, body, action] = direct
      ? [t('confirm.delete_title', { name: room.name }), t('confirm.delete_body', { name: room.name }), t('confirm.delete')]
      : [t('confirm.leave_title', { room: room.name }), t('confirm.leave_body'), t('confirm.leave')];
    Alert.alert(title, body, [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: action,
        style: 'destructive',
        onPress: () => {
          leaveRoom(store, id, room.room_id, direct);
          done();
        },
      },
    ]);
  };

  return (
    <>
      {header}
      <ListSection>
        <ListRow
          testID="room-menu-notifications"
          icon="bell"
          title={t('notify.heading')}
          value={room.custom_notifications ? modeLabel(room.notifications) : t('notify.default')}
          onPress={() => setPage({ kind: 'notifications' })}
        />
        {room.unread > 0 || room.mentions > 0 ? (
          <>
            <Divider inset={52} />
            <ListRow
              testID="room-menu-read"
              icon="markRead"
              title={t('mobile.shell.mark_read')}
              chevron={false}
              onPress={() => {
                void markRoomRead(store, id, room.room_id);
                done();
              }}
            />
          </>
        ) : null}
      </ListSection>
      {inScope ? (
        <ListSection>
          <ListRow
            testID="room-menu-move"
            icon="folder"
            title={t('mobile.shell.move_to_section')}
            value={own.find((s) => s.id === current)?.name}
            onPress={() => setPage({ kind: 'move' })}
          />
          {current ? (
            <>
              <Divider inset={52} />
              <ListRow
                testID="room-menu-unsection"
                icon="folderRemove"
                title={t('mobile.shell.remove_from_section')}
                chevron={false}
                onPress={() => {
                  moveRoomToSection(store, id, scope, room.room_id, direct ? DIRECT : ROOMS);
                  done();
                }}
              />
            </>
          ) : null}
        </ListSection>
      ) : null}
      <ListSection>
        <ListRow
          testID="room-menu-leave"
          icon={direct ? 'trash' : 'leave'}
          title={direct ? t('room_menu.delete_chat') : t('room_menu.leave')}
          danger
          chevron={false}
          onPress={confirmLeave}
        />
      </ListSection>
    </>
  );
}

/** The built-in sections and the unread group are named by the app, the user's by the user. */
function sectionTitle(account: AccountData, scope: Scope, id: string): string {
  if (id === ROOMS) return t('sidebar.rooms');
  if (id === DIRECT) return t('sidebar.direct');
  if (id === 'unread') return t('overview.unread');
  return account.sidebar.scopes[scopeKey(scope)]?.sections.find((s) => s.id === id)?.name ?? '';
}

function SectionPage({ account, id, setPage }: { account: AccountData; id: string; setPage: (page: Page) => void }) {
  const store = useAppStore();
  const palette = usePalette();
  const accountId: AccountId = account.summary.id;
  const scope = account.scope;
  const collapsed = account.sidebar.scopes[scopeKey(scope)]?.collapsed.includes(id) ?? false;
  const own = !isBuiltin(id) && id !== 'unread';
  return (
    <>
      <View style={styles.header}>
        <Text style={[font.header, styles.grow, { color: palette.text }]} numberOfLines={1}>
          {sectionTitle(account, scope, id)}
        </Text>
      </View>
      <ListSection>
        <ListRow
          testID="section-toggle"
          icon={collapsed ? 'chevronDown' : 'chevronRight'}
          title={collapsed ? t('sections.expand') : t('sections.collapse')}
          chevron={false}
          onPress={() => {
            toggleSection(store, accountId, scope, id);
            router.back();
          }}
        />
        {own ? (
          <>
            <Divider inset={52} />
            <ListRow
              testID="section-rename"
              icon="pencil"
              title={t('sections.rename')}
              onPress={() => setPage({ kind: 'name', section: id, roomId: null })}
            />
            <Divider inset={52} />
            <ListRow
              testID="section-delete"
              icon="trash"
              title={t('sections.delete')}
              danger
              chevron={false}
              onPress={() => {
                deleteSection(store, accountId, scope, id);
                router.back();
              }}
            />
          </>
        ) : null}
      </ListSection>
    </>
  );
}

function SectionName({ account, page }: { account: AccountData; page: Extract<Page, { kind: 'name' }> }) {
  const store = useAppStore();
  const palette = usePalette();
  const accountId = account.summary.id;
  const scope = account.scope;
  const [name, setName] = useState(() => (page.section ? sectionTitle(account, scope, page.section) : ''));
  const submit = () => {
    if (!name.trim()) return;
    if (page.section) renameSection(store, accountId, scope, page.section, name);
    else addSection(store, accountId, scope, name, page.roomId ?? undefined);
    router.back();
  };
  return (
    <View style={styles.form}>
      <Text style={[font.header, { color: palette.text }]}>{page.section ? t('sections.rename_title') : t('sections.new_title')}</Text>
      <TextField
        testID="section-name"
        autoFocus
        value={name}
        onChangeText={setName}
        placeholder={t('sections.name_placeholder')}
        returnKeyType="done"
        onSubmitEditing={submit}
      />
      <Button
        testID="section-save"
        title={page.section ? t('common.save') : t('create.create')}
        disabled={!name.trim()}
        onPress={submit}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 32 },
  header: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 20, paddingTop: 20 },
  grow: { flex: 1 },
  form: { padding: 20, gap: 16 },
});
