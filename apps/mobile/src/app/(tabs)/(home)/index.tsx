// Home: the desktop's sidebar as Slack's mobile app lays it out. The title is the scope (Home or
// a space) and opens the space picker; the header's buttons are the bell (Activity) and the
// create menu. Under "Jump to…" and the banners: the conversations wanting attention, then the
// scope's sections, in rows as compact as the desktop's.

import type { Invitation, RoomSummary } from '@better-matrix/matrix-native/types';
import { router, Stack } from 'expo-router';
import { useMemo } from 'react';
import { SectionList, StyleSheet, Text, View } from 'react-native';

import { useNotificationOpener } from '../../../engine/notifications';
import { t } from '../../../i18n';
import { routes } from '../../../routes';
import { toggleSection } from '../../../state/actions/shell';
import { useAppStore, useStore } from '../../../state/react';
import { hasUnread, isBuiltin, lastMessage, ROOMS, sameScope, scopeKey, sections, unreadRooms, wantsAttention } from '../../../state/sections';
import { activeAccount } from '../../../state/selectors';
import type { AccountData } from '../../../state/types';
import { font } from '../../../theme/palette';
import { EndedSessions } from '../../../ui/accounts/EndedSessions';
import { SecurityBanner } from '../../../ui/accounts/SecurityBanner';
import { CallBar } from '../../../ui/call/CallBar';
import { EmptyState, Touchable } from '../../../ui/components';
import { Icon } from '../../../ui/icons';
import { CreateMenu } from '../../../ui/shell/CreateMenu';
import { InvitationRow } from '../../../ui/shell/InvitationRow';
import { JumpTo } from '../../../ui/shell/JumpTo';
import { ScopeTitle } from '../../../ui/shell/ScopeTitle';
import { RoomRow } from '../../../ui/shell/RoomRow';
import { useNow } from '../../../ui/shell/useNow';
import { SectionHeader } from '../../../ui/shell/SectionHeader';
import { usePalette } from '../../../ui/theme';

/** The group of conversations wanting attention, above the sections; collapsible like them. */
const UNREAD = 'unread';
/** Invitations waiting for an answer, first of all, as the desktop's fixed section. */
const INVITATIONS = 'invitations';

interface ListSection {
  id: string;
  title: string;
  collapsed: boolean;
  /** One of the user's own sections (empty ones say how to fill them). */
  own: boolean;
  attention: number;
  /** The rooms (or invitations) shown: none while collapsed. */
  data: Array<RoomSummary | Invitation>;
}

function isInvitation(item: RoomSummary | Invitation): item is Invitation {
  return 'inviter' in item;
}

function listSections(account: AccountData, now: number): ListSection[] {
  const scope = account.scope;
  const collapsed = new Set(account.sidebar.scopes[scopeKey(scope)]?.collapsed ?? []);
  const out: ListSection[] = [];
  if (account.invitations.length > 0) {
    const closed = collapsed.has(INVITATIONS);
    out.push({
      id: INVITATIONS,
      title: t('invitations.heading', { count: account.invitations.length }),
      collapsed: closed,
      own: false,
      attention: 0,
      data: closed ? [] : account.invitations,
    });
  }
  const unread = unreadRooms(account, scope);
  if (unread.length > 0) {
    const closed = collapsed.has(UNREAD);
    out.push({
      id: UNREAD,
      title: t('overview.unread'),
      collapsed: closed,
      own: false,
      attention: unread.length,
      data: closed ? [] : unread,
    });
  }
  for (const view of sections(account, scope, now)) {
    // The built-in sections show once they hold something; the user's always do.
    if (isBuiltin(view.id) && view.rooms.length === 0) continue;
    out.push({
      id: view.id,
      title: view.name ?? (view.id === ROOMS ? t('sidebar.rooms') : t('sidebar.direct')),
      collapsed: view.collapsed,
      own: !isBuiltin(view.id),
      attention: view.rooms.filter(wantsAttention).length,
      data: view.collapsed ? [] : view.rooms,
    });
  }
  return out;
}

export default function HomeScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useStore(activeAccount);
  // Another scope of this account has something new: the title's chevron gets a dot.
  const elsewhere = useStore((s) => {
    const a = activeAccount(s);
    if (!a) return false;
    const scopes = ['home' as const, ...a.spaces.map((x) => ({ space: x.room_id }))];
    return scopes.some((scope) => !sameScope(scope, a.scope) && hasUnread(a, scope));
  });
  useNotificationOpener();
  const now = useNow();
  const list = useMemo(() => (account ? listSections(account, now) : []), [account, now]);
  if (!account) return null;
  const id = account.summary.id;
  const scope = account.scope;
  const title =
    scope === 'home' ? t('common.home') : (account.spaces.find((s) => s.room_id === scope.space)?.name ?? t('common.home'));

  const header = (
    <View>
      <EndedSessions />
      <SecurityBanner />
      <CallBar />
      <JumpTo />
    </View>
  );

  return (
    <>
      <Stack.Screen options={{ title, headerTitle: () => <ScopeTitle title={title} unread={elsewhere} /> }} />
      <CreateMenu account={id} scope={scope} />
      {!account.roomsLoaded ? (
        <View style={styles.fill}>
          {header}
          <EmptyState loading title={t('sidebar.syncing')} />
        </View>
      ) : (
        <SectionList
          testID="room-list"
          style={styles.fill}
          contentInsetAdjustmentBehavior="automatic"
          sections={list}
          keyExtractor={(room, index) => `${room.room_id}-${index}`}
          stickySectionHeadersEnabled={false}
          ListHeaderComponent={header}
          ListEmptyComponent={<EmptyState icon="chat" title={t('sidebar.empty')} />}
          renderSectionHeader={({ section }) => (
            <SectionHeader
              id={section.id}
              title={section.title}
              collapsed={section.collapsed}
              count={section.attention}
              onToggle={() => toggleSection(store, id, scope, section.id)}
              onMenu={section.id === INVITATIONS ? undefined : () => router.push(routes.sectionMenu(id, section.id))}
            />
          )}
          renderSectionFooter={({ section }) =>
            section.own && !section.collapsed && section.data.length === 0 ? (
              <Text style={[font.meta, styles.hint, { color: palette.textMuted }]}>{t('mobile.shell.section_empty')}</Text>
            ) : null
          }
          renderItem={({ item }) =>
            isInvitation(item) ? (
              <InvitationRow account={id} invitation={item} />
            ) : (
              <RoomRow account={id} room={item} last={lastMessage(account, item.room_id)} now={now} />
            )
          }
          ListFooterComponent={
            <Touchable
              testID="add-section"
              accessibilityRole="button"
              onPress={() => router.push(routes.sectionMenu(id))}
              style={styles.addSection}
            >
              <Icon name="plus" size={16} color={palette.textMuted} />
              <Text style={[font.nav, { color: palette.textMuted }]}>{t('sections.new')}</Text>
            </Touchable>
          }
        />
      )}
    </>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  hint: { paddingHorizontal: 18, paddingVertical: 6 },
  addSection: { flexDirection: 'row', alignItems: 'center', gap: 10, paddingHorizontal: 18, paddingVertical: 16, marginBottom: 24 },
});
