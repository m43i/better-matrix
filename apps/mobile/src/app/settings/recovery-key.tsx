// Setting up recovery (or with `reset` replacing the key), then the new recovery key, shown once:
// in monospace to copy or share, until the user says it is saved.

import * as Clipboard from 'expo-clipboard';
import { router, Stack, useLocalSearchParams } from 'expo-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Platform, Share, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { forgetRecoveryKey, recoveryKeyFor, setUpRecovery } from '../../state/actions/security';
import { useAppStore, useStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button } from '../../ui/components';
import { NoAccount, SettingsPage, useSettingsAccount } from '../../ui/settings/common';
import { usePalette } from '../../ui/theme';

export default function RecoveryKeyScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const { reset } = useLocalSearchParams<{ reset?: string }>();
  const id = account?.summary.id ?? null;
  const key = useStore((s) => (id ? recoveryKeyFor(s, id) : null));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  const start = useCallback(() => {
    if (!id) return;
    setError(null);
    setBusy(true);
    setUpRecovery(store, id, reset === '1')
      .catch((cause: unknown) => setError(cause instanceof Error ? cause.message : String(cause)))
      .finally(() => setBusy(false));
  }, [store, id, reset]);

  // Opening the page sets up recovery, once; the key goes once the page does.
  const asked = useRef(false);
  useEffect(() => {
    if (!asked.current) {
      asked.current = true;
      start();
    }
  }, [start]);
  useEffect(() => () => forgetRecoveryKey(store), [store]);

  const title = t('recovery.new_title');

  if (!account) return <NoAccount title={title} />;

  const copy = () => {
    if (!key) return;
    void Clipboard.setStringAsync(key).then(() => setCopied(true));
  };

  const done = () => {
    forgetRecoveryKey(store);
    router.back();
  };

  return (
    <SettingsPage title={title} testID="recovery-key">
      {/* A key on screen leaves only through "I've saved it". */}
      <Stack.Screen options={{ gestureEnabled: key === null, headerBackVisible: key === null }} />
      <View style={styles.page}>
        {key ? (
          <>
            <Text style={[font.body, { color: palette.textMuted }]}>{t('recovery.new_hint')}</Text>
            <View
              style={[
                styles.key,
                { backgroundColor: palette.surface, borderColor: palette.divider },
              ]}
            >
              <Text
                testID="recovery-key-shown"
                selectable
                style={[font.body, styles.mono, { color: palette.text }]}
              >
                {key}
              </Text>
            </View>
            <View style={styles.buttons}>
              <View style={styles.grow}>
                <Button
                  testID="recovery-copy"
                  title={copied ? t('mobile.accounts.copied') : t('recovery.copy')}
                  icon={copied ? 'check' : 'copy'}
                  kind="secondary"
                  onPress={copy}
                />
              </View>
              <View style={styles.grow}>
                <Button
                  testID="recovery-share"
                  title={t('mobile.accounts.share')}
                  icon="share"
                  kind="secondary"
                  onPress={() => void Share.share({ message: key })}
                />
              </View>
            </View>
            <Button testID="recovery-saved" title={t('recovery.saved')} onPress={done} />
          </>
        ) : error ? (
          <>
            <Text style={[font.body, { color: palette.danger }]}>{error}</Text>
            <Button testID="recovery-retry" title={t('common.try_again')} onPress={start} />
          </>
        ) : (
          <View style={styles.busy}>
            {busy ? <ActivityIndicator color={palette.textMuted} /> : null}
            <Text style={[font.body, { color: palette.textMuted }]}>
              {t('mobile.accounts.making_key')}
            </Text>
          </View>
        )}
      </View>
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  page: { padding: 20, gap: 16 },
  key: { borderRadius: 10, borderWidth: StyleSheet.hairlineWidth, padding: 16 },
  mono: { fontFamily: Platform.select({ ios: 'Menlo', default: 'monospace' }), letterSpacing: 0.5 },
  buttons: { flexDirection: 'row', gap: 8 },
  grow: { flex: 1 },
  busy: { flexDirection: 'row', alignItems: 'center', gap: 12 },
});
