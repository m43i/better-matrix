// Exporting message keys: a passphrase, typed twice, protects the file, which then goes to the
// system share sheet (save it to Files or a drive, or send it to another app).

import { router } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { confirmationProblem } from '../../state/actions/security';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button, TextField } from '../../ui/components';
import { NoAccount, SettingsPage, useSettingsAccount } from '../../ui/settings/common';
import { exportAndShare } from '../../ui/settings/keys';
import { usePalette } from '../../ui/theme';

export default function ExportKeysScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const [passphrase, setPassphrase] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const title = t('mobile.accounts.export_keys');

  if (!account) return <NoAccount title={title} />;
  const id = account.summary.id;

  const submit = () => {
    if (exporting) return;

    switch (confirmationProblem(passphrase, confirmation)) {
      case 'empty':
        setError(t('mobile.accounts.passphrase_empty'));

        return;
      case 'mismatch':
        setError(t('mobile.accounts.passphrase_mismatch'));

        return;
      case null:
        break;
    }

    setError(null);
    setExporting(true);
    exportAndShare(store, id, passphrase).then(
      () => router.back(),
      (cause: unknown) => {
        setExporting(false);
        setError(cause instanceof Error ? cause.message : String(cause));
      },
    );
  };

  return (
    <SettingsPage title={title} testID="export-keys-page">
      <View style={styles.form}>
        <Text style={[font.body, { color: palette.textMuted }]}>
          {t('mobile.accounts.export_hint')}
        </Text>
        <TextField
          testID="export-passphrase"
          label={t('mobile.accounts.passphrase')}
          value={passphrase}
          onChangeText={setPassphrase}
          secureTextEntry
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="new-password"
          textContentType="newPassword"
          returnKeyType="next"
        />
        <TextField
          testID="export-passphrase-confirm"
          label={t('mobile.accounts.passphrase_confirm')}
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
          testID="export-submit"
          title={exporting ? t('mobile.accounts.exporting') : t('mobile.accounts.export_action')}
          icon="upload"
          busy={exporting}
          onPress={submit}
        />
      </View>
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  form: { padding: 20, gap: 16 },
});
