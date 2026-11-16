// Verifying this session with the recovery key, as the desktop's recovery key dialog; the way to
// resetting the identity for a key that is lost.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { router, Stack, useLocalSearchParams } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';

import { t } from '../i18n';
import { routes } from '../routes';
import { recover } from '../state/actions/security';
import { useAppStore } from '../state/react';
import { font } from '../theme/palette';
import { Button, TextField } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';

export default function RecoveryScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const { account } = useLocalSearchParams<{ account: AccountId }>();
  const [key, setKey] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const submit = () => {
    if (submitting || !key.trim()) return;
    setError(null);
    setSubmitting(true);
    recover(store, account, key).then(
      () => router.back(),
      (e: unknown) => {
        setSubmitting(false);
        setError(e instanceof Error ? e.message : String(e));
      },
    );
  };

  return (
    <>
      <Stack.Screen
        options={{
          title: t('recovery.enter_title'),
          headerShown: true,
          headerLeft: () => (
            <Button testID="recovery-cancel" title={t('common.cancel')} kind="plain" onPress={() => router.back()} />
          ),
        }}
      />
      <KeyboardAwareScrollView
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.page}
        keyboardShouldPersistTaps="handled"
        bottomOffset={24}
      >
        <View style={styles.heading}>
          <Icon name="key" size={36} color={palette.accent} />
          <Text style={[font.body, { color: palette.textMuted, textAlign: 'center' }]}>{t('recovery.enter_hint')}</Text>
        </View>
        <TextField
          testID="recovery-key"
          value={key}
          onChangeText={setKey}
          placeholder={t('recovery.key_placeholder')}
          secureTextEntry
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="off"
          autoFocus
          returnKeyType="done"
          onSubmitEditing={submit}
          error={error}
        />
        <Button
          testID="recovery-submit"
          title={submitting ? t('recovery.verifying') : t('recovery.verify')}
          busy={submitting}
          disabled={!key.trim()}
          onPress={submit}
        />
        <Button
          testID="recovery-forgot"
          title={t('reset.forgot')}
          kind="plain"
          onPress={() => router.replace(routes.settingsReset(account))}
        />
      </KeyboardAwareScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  page: { padding: 24, gap: 18 },
  heading: { alignItems: 'center', gap: 12 },
});
