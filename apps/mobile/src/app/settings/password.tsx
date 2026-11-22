// Changing a password account's password: the current one, then the new one twice. The server
// signs the account's other sessions out once it changed. OAuth accounts change theirs on the
// account page instead (Account & security opens it).

import { router } from 'expo-router';
import { useState } from 'react';
import { Alert, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { changePassword, confirmationProblem } from '../../state/actions/security';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button, TextField } from '../../ui/components';
import { NoAccount, SettingsPage, useSettingsAccount } from '../../ui/settings/common';
import { usePalette } from '../../ui/theme';

export default function PasswordScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [changing, setChanging] = useState(false);
  const title = t('mobile.accounts.password_title');

  if (!account) return <NoAccount title={title} />;
  const id = account.summary.id;

  const submit = () => {
    if (changing) return;
    const problem = current === '' ? 'empty' : confirmationProblem(next, confirmation);

    if (problem !== null) {
      setError(
        problem === 'empty'
          ? t('mobile.accounts.password_empty')
          : t('mobile.accounts.password_mismatch'),
      );

      return;
    }

    setError(null);
    setChanging(true);
    changePassword(store, id, current, next).then(
      () => {
        router.back();
        Alert.alert(t('mobile.accounts.password_changed'));
      },
      (cause: unknown) => {
        setChanging(false);
        setError(cause instanceof Error ? cause.message : String(cause));
      },
    );
  };

  return (
    <SettingsPage title={title} testID="password-page">
      <View style={styles.form}>
        <Text style={[font.body, { color: palette.textMuted }]}>
          {t('mobile.accounts.password_hint')}
        </Text>
        <TextField
          testID="password-current"
          label={t('mobile.accounts.password_current')}
          value={current}
          onChangeText={setCurrent}
          secureTextEntry
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="current-password"
          textContentType="password"
          returnKeyType="next"
        />
        <TextField
          testID="password-new"
          label={t('mobile.accounts.password_new')}
          value={next}
          onChangeText={setNext}
          secureTextEntry
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="new-password"
          textContentType="newPassword"
          returnKeyType="next"
        />
        <TextField
          testID="password-confirm"
          label={t('mobile.accounts.password_confirm')}
          value={confirmation}
          onChangeText={setConfirmation}
          secureTextEntry
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="new-password"
          textContentType="newPassword"
          returnKeyType="done"
          onSubmitEditing={submit}
          error={error}
        />
        <Button
          testID="password-submit"
          title={changing ? t('mobile.accounts.password_changing') : title}
          busy={changing}
          onPress={submit}
        />
      </View>
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  form: { padding: 20, gap: 16 },
});
