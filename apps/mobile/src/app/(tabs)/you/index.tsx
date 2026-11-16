// You (Slack's "You" tab): the account on screen, every signed-in account to switch to, the
// settings pages, and signing out. The mobile form of the desktop's account menu.

import { Stack, router } from 'expo-router';
import { Alert, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../../../i18n';
import { routes } from '../../../routes';
import { removeEnded, signOut } from '../../../state/actions/accounts';
import { setActive } from '../../../state/actions/core';
import { shallow, useAppStore, useStore } from '../../../state/react';
import { activeAccount } from '../../../state/selectors';
import { font } from '../../../theme/palette';
import { Avatar } from '../../../ui/Avatar';
import { Divider, ListRow, ListSection } from '../../../ui/components';
import { SettingsRows } from '../../../ui/settings/SettingsRows';
import { usePalette } from '../../../ui/theme';

export default function YouScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const active = useStore((s) => activeAccount(s)?.summary);
  const accounts = useStore((s) => s.accounts.map((a) => a.summary), shallow);
  const suspended = useStore((s) => s.ended.filter((e) => e.kept).map((e) => e.account), shallow);

  const confirmSignOut = () => {
    if (!active) return;
    Alert.alert(
      t('accounts.sign_out', { name: active.display_name }),
      t('mobile.accounts.sign_out_hint'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('sessions.sign_out'),
          style: 'destructive',
          onPress: () => signOut(store, active.id),
        },
      ],
    );
  };

  return (
    <>
      <Stack.Screen options={{ title: t('mobile.tabs.you') }} />
      <ScrollView
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.content}
        contentInsetAdjustmentBehavior="automatic"
      >
        {active ? (
          <View style={styles.header} testID="you-header">
            <Avatar
              account={active.id}
              id={active.id}
              name={active.display_name}
              media={active.avatar}
              size={72}
            />
            <View style={styles.headerText}>
              <Text style={[font.title, { color: palette.text, fontSize: 24 }]} numberOfLines={1}>
                {active.display_name}
              </Text>
              <Text style={[font.body, { color: palette.textMuted }]} numberOfLines={1} selectable>
                {active.id}
              </Text>
            </View>
          </View>
        ) : null}

        <ListSection title={t('accounts.heading')}>
          {accounts.map((summary) => (
            <View key={summary.id}>
              <ListRow
                testID={`switch-${summary.id}`}
                leading={
                  <Avatar
                    account={summary.id}
                    id={summary.id}
                    name={summary.display_name}
                    media={summary.avatar}
                    size={32}
                  />
                }
                title={summary.display_name}
                subtitle={summary.id}
                selected={summary.id === active?.id}
                chevron={false}
                onPress={() => setActive(store, summary.id)}
              />
              <Divider inset={62} />
            </View>
          ))}
          {suspended.map((account) => (
            <View key={account}>
              <ListRow
                testID={`resume-${account}`}
                icon="user"
                title={account}
                subtitle={t('accounts.signed_out')}
                onPress={() =>
                  Alert.alert(t('session_ended.title', { account }), t('session_ended.kept_hint'), [
                    { text: t('common.cancel'), style: 'cancel' },
                    {
                      text: t('session_ended.remove'),
                      style: 'destructive',
                      onPress: () => removeEnded(store, account),
                    },
                    {
                      text: t('session_ended.sign_in'),
                      onPress: () => router.push(routes.signIn(account)),
                    },
                  ])
                }
              />
              <Divider inset={52} />
            </View>
          ))}
          <ListRow
            testID="add-account"
            icon="userAdd"
            title={t('accounts.add')}
            onPress={() => router.push(routes.signIn())}
          />
        </ListSection>

        <SettingsRows />

        {active ? (
          <ListSection>
            <ListRow
              testID="sign-out"
              icon="signOut"
              title={t('accounts.sign_out', { name: active.display_name })}
              danger
              onPress={confirmSignOut}
            />
          </ListSection>
        ) : null}
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 40 },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 16,
    paddingHorizontal: 20,
    paddingTop: 16,
  },
  headerText: { flex: 1, gap: 2 },
});
