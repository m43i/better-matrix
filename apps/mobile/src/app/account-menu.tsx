// The account menu, opened from the avatar in the tab bar, as the desktop's account menu: every
// signed-in account to switch to (a dot when it has something new), adding one, settings, and
// signing out of the account on screen.

import type { AccountSummary } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { Alert, ScrollView, StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { signOut } from '../state/actions/accounts';
import { setActive } from '../state/actions/core';
import { shallow, useAppStore, useStore } from '../state/react';
import { hasUnread } from '../state/sections';
import type { State } from '../state/types';
import { Divider, ListRow, ListSection } from '../ui/components';
import { AccountAvatar, WithDot } from '../ui/shell/AccountButton';
import { usePalette } from '../ui/theme';

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
  const active = useStore((s) => s.active);
  const current = accounts.find((a) => a.summary.id === active)?.summary;

  const confirmSignOut = () => {
    if (!current) return;
    Alert.alert(
      t('accounts.sign_out', { name: current.display_name }),
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
      <ListSection title={t('accounts.heading')}>
        {accounts.map(({ summary, unread }, i) => (
          <View key={summary.id}>
            {i > 0 ? <Divider inset={66} /> : null}
            <ListRow
              testID={`switch-${summary.id}`}
              title={summary.display_name || summary.id}
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
              title={t('accounts.sign_out', { name: current.display_name || current.id })}
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
});
