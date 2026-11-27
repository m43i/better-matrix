// The account menu, opened from the avatar in the tab bar, as the desktop's account menu: the
// status of the account on screen, every signed-in account to switch to (a dot when it has
// something new), adding one, its saved messages, settings, and signing out of the account on
// screen.

import type { AccountSummary, Status } from '@better-matrix/matrix-native/types';
import { personName } from '../state/names';
import { router } from 'expo-router';
import { Alert, ScrollView, StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { setStatus, signOut } from '../state/actions/accounts';
import { setActive } from '../state/actions/core';
import { shallow, useAppStore, useStore } from '../state/react';
import { sendTimeText } from '../state/schedule';
import { hasUnread } from '../state/sections';
import { activeAccount } from '../state/selectors';
import { statusMessageDeadline } from '../state/statusMessage';
import type { State } from '../state/types';
import { Divider, ListRow, ListSection } from '../ui/components';
import { AccountAvatar, StatusDot, WithDot } from '../ui/shell/AccountButton';
import { useNow } from '../ui/shell/useNow';
import { usePalette } from '../ui/theme';

/** The statuses to choose from, in order, with their stable IDs and titles' catalog keys. */
const STATUSES = [
  { status: 'Online', id: 'online', title: 'status.online' },
  { status: 'Away', id: 'away', title: 'status.away' },
  { status: 'Hidden', id: 'hidden', title: 'status.hidden' },
] as const satisfies readonly { status: Status; id: string; title: string }[];

interface AccountEntry {
  summary: AccountSummary;
  unread: boolean;
}

function accountEntries(s: State): AccountEntry[] {
  return s.accounts.map((a) => ({ summary: a.summary, unread: hasUnread(a, 'home') }));
}

function sameEntries(a: AccountEntry[], b: AccountEntry[]): boolean {
  return a.length === b.length && a.every((x, i) => shallow(x, b[i]));
}

/** Leaves this sheet for another screen. */
function go(href: Parameters<typeof router.push>[0]) {
  router.back();
  router.push(href);
}

export default function AccountMenuScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const accounts = useStore(accountEntries, sameEntries);
  const reverse = useStore((s) => s.settings.reverseDisplayNames);
  const active = useStore((s) => s.active);
  const current = accounts.find((a) => a.summary.id === active)?.summary;
  const status = useStore((s) => s.accounts.find((a) => a.summary.id === s.active)?.status);
  const now = useNow();
  const message = useStore((s) => activeAccount(s)?.statusMessage);

  const clearAt = useStore((s) => {
    const data = activeAccount(s);

    return data ? statusMessageDeadline(data, now) : null;
  });

  const confirmSignOut = () => {
    if (!current) return;
    Alert.alert(
      t('accounts.sign_out', { name: personName(current.display_name || current.id, reverse) }),
      t('mobile.accounts.sign_out_hint'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('sessions.sign_out'),
          style: 'destructive',
          onPress: () => {
            router.back();
            signOut(store, current.id);
          },
        },
      ],
    );
  };

  return (
    <ScrollView style={{ backgroundColor: palette.bg }} contentContainerStyle={styles.content}>
      {current ? (
        <ListSection>
          <ListRow
            testID="open-status-message"
            icon="sticker"
            title={message ?? t('status_message.set')}
            subtitle={
              clearAt === null
                ? undefined
                : t('status_message.expires_at', { time: sendTimeText(clearAt, now) })
            }
            onPress={() => router.push(routes.statusMessage(current.id))}
          />
        </ListSection>
      ) : null}
      {current && status ? (
        <ListSection title={t('status.heading')}>
          {STATUSES.map((choice, i) => (
            <View key={choice.id}>
              {i > 0 ? <Divider inset={52} /> : null}
              <ListRow
                testID={`status-${choice.id}`}
                title={t(choice.title)}
                subtitle={choice.status === 'Hidden' ? t('status.hidden_hint') : undefined}
                leading={
                  <View style={styles.status}>
                    <StatusDot status={choice.status} size={12} ring={palette.surface} />
                  </View>
                }
                selected={choice.status === status}
                chevron={false}
                onPress={() => {
                  setStatus(store, current.id, choice.status);
                  router.back();
                }}
              />
            </View>
          ))}
        </ListSection>
      ) : null}
      <ListSection title={t('accounts.heading')}>
        {accounts.map(({ summary, unread }, i) => (
          <View key={summary.id}>
            {i > 0 ? <Divider inset={66} /> : null}
            <ListRow
              testID={`switch-${summary.id}`}
              title={personName(summary.display_name || summary.id, reverse)}
              subtitle={summary.id}
              leading={
                <WithDot on={summary.id !== active && unread} size={36}>
                  <AccountAvatar summary={summary} size={36} />
                </WithDot>
              }
              selected={summary.id === active}
              chevron={false}
              onPress={() => {
                setActive(store, summary.id);
                router.back();
              }}
            />
          </View>
        ))}
        <Divider inset={66} />
        <ListRow
          testID="account-menu-add"
          icon="userAdd"
          title={t('accounts.add')}
          onPress={() => go(routes.signIn())}
        />
      </ListSection>
      <ListSection>
        {current ? (
          <>
            <ListRow
              testID="account-menu-saved"
              icon="bookmark"
              title={t('saved.title')}
              onPress={() => go(routes.saved(current.id))}
            />
            <Divider inset={52} />
          </>
        ) : null}
        <ListRow
          testID="account-menu-settings"
          icon="settings"
          title={t('settings.title')}
          onPress={() => go(routes.settings())}
        />
        {current ? (
          <>
            <Divider inset={52} />
            <ListRow
              testID="account-menu-sign-out"
              icon="signOut"
              title={t('accounts.sign_out', {
                name: personName(current.display_name || current.id, reverse),
              })}
              danger
              chevron={false}
              onPress={confirmSignOut}
            />
          </>
        ) : null}
      </ListSection>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  status: { width: 22, alignItems: 'center' },
});
