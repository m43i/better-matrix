// Accounts whose session the server ended, each with "Sign in again" and a way to forget it, as
// the desktop's session-ended dialog; and saved accounts still signing back in. Owned by the
// accounts area; shown at the top of Home.

import { router } from 'expo-router';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { removeEnded } from '../../state/actions/accounts';
import { shallow, useAppStore, useStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function EndedSessions() {
  const store = useAppStore();
  const palette = usePalette();
  const ended = useStore((s) => s.ended, shallow);
  const restoring = useStore((s) => s.restoring, shallow);

  if (ended.length === 0 && restoring.length === 0) return null;

  return (
    <View style={styles.list} testID="ended-sessions">
      {restoring.map((account) => (
        <View
          key={account}
          style={[
            styles.card,
            styles.restoring,
            { backgroundColor: palette.surface, borderColor: palette.divider },
          ]}
        >
          <ActivityIndicator color={palette.textMuted} />
          <Text style={[font.body, { color: palette.textMuted, flex: 1 }]} numberOfLines={1}>
            {t('mobile.accounts.restoring', { account })}
          </Text>
        </View>
      ))}
      {ended.map(({ account, kept }) => (
        <View
          key={account}
          testID={`ended-${account}`}
          style={[styles.card, { backgroundColor: palette.surface, borderColor: palette.divider }]}
        >
          <View style={styles.title}>
            <Icon name="signOut" size={20} color={palette.warning} />
            <Text style={[font.bodyStrong, { color: palette.text, flex: 1 }]} numberOfLines={2}>
              {t('session_ended.title', { account })}
            </Text>
          </View>
          <Text style={[font.meta, { color: palette.textMuted }]}>
            {kept ? t('session_ended.kept_hint') : t('session_ended.hint')}
          </Text>
          <View style={styles.buttons}>
            <Button
              testID={`ended-remove-${account}`}
              title={kept ? t('session_ended.remove') : t('common.dismiss')}
              kind="plain"
              onPress={() => removeEnded(store, account)}
            />
            <Button
              testID={`ended-sign-in-${account}`}
              title={t('session_ended.sign_in')}
              onPress={() => router.push(routes.signIn(account))}
            />
          </View>
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { gap: 8, marginHorizontal: 16, marginVertical: 8 },
  card: { borderRadius: 12, borderWidth: StyleSheet.hairlineWidth, padding: 14, gap: 8 },
  restoring: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  title: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  buttons: { flexDirection: 'row', justifyContent: 'flex-end', gap: 8, flexWrap: 'wrap' },
});
