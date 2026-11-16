// Resetting the identity, when the recovery key and every verified session are lost: what it
// keeps and costs first, then the reset (with the password, or approved on an OAuth account's
// page), then setting up recovery.

import { router, Stack } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { type ResetStep, resetIdentity } from '../../state/actions/security';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { openInBrowser } from '../../ui/accounts/browser';
import { usePrompt } from '../../ui/accounts/Prompt';
import { Button } from '../../ui/components';
import { Icon, type IconName } from '../../ui/icons';
import { NoAccount, SettingsPage, StatusLine, useSettingsAccount } from '../../ui/settings/common';
import { usePalette } from '../../ui/theme';

export default function ResetScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const prompt = usePrompt();
  const [step, setStep] = useState<ResetStep>({ type: 'warning', error: null });
  const running = useRef<ResetStep>(step);
  useEffect(() => {
    running.current = step;
  }, [step]);

  // Leaving stops a reset that waits for its approval.
  useEffect(
    () => () => {
      const last = running.current;

      if (last.type === 'resetting' && last.request !== null) {
        store.send({ type: 'Cancel', data: last.request }, { quiet: true });
      }
    },
    [store],
  );

  if (!account) return <NoAccount title={t('reset.title_short')} />;
  const id = account.summary.id;

  const confirm = () => {
    void resetIdentity(store, id, {
      askPassword: (error) =>
        prompt.ask({
          title: t('reset.password_title'),
          message: t('reset.password_hint'),
          error,
          confirm: t('reset.confirm'),
          secure: true,
          destructive: true,
          placeholder: t('sign_in.password'),
        }),
      openUrl: (url) => openInBrowser(store, url),
      onStep: setStep,
    });
  };

  const cancel = () => {
    if (step.type === 'resetting' && step.request !== null) {
      store.send({ type: 'Cancel', data: step.request }, { quiet: true });
    }

    router.back();
  };

  const title =
    step.type === 'done'
      ? t('reset.done_title')
      : step.type === 'resetting' && step.url
        ? t('reset.approve_title')
        : t('reset.title');

  return (
    <SettingsPage title={t('reset.title_short')} testID="reset-identity">
      <Stack.Screen
        options={{
          gestureEnabled: step.type !== 'resetting',
          headerBackVisible: step.type !== 'resetting',
        }}
      />
      <View style={styles.page}>
        <Text style={[font.title, { color: palette.text }]}>{title}</Text>
        {step.type === 'warning' ? (
          <>
            <Text style={[font.body, { color: palette.textMuted }]}>{t('reset.warning')}</Text>
            <Consequence icon="check" color={palette.success} text={t('reset.keeps')} />
            <Consequence icon="alert" color={palette.warning} text={t('reset.loses_history')} />
            <Consequence icon="alert" color={palette.warning} text={t('reset.verify_again')} />
            {step.error ? (
              <Text style={[font.body, { color: palette.danger }]}>{step.error}</Text>
            ) : null}
            <Button
              testID="reset-confirm"
              title={step.error ? t('common.try_again') : t('reset.confirm')}
              kind="danger"
              onPress={confirm}
            />
            <Button
              testID="reset-cancel"
              title={t('common.cancel')}
              kind="secondary"
              onPress={cancel}
            />
          </>
        ) : step.type === 'resetting' ? (
          <>
            {step.url ? (
              <Text style={[font.body, { color: palette.textMuted }]}>{t('reset.approve')}</Text>
            ) : (
              <View style={styles.busy}>
                <ActivityIndicator color={palette.textMuted} />
                <Text style={[font.body, { color: palette.textMuted }]}>
                  {t('reset.resetting')}
                </Text>
              </View>
            )}
            {step.url ? (
              <Button
                testID="reset-reopen"
                title={t('reset.open_again')}
                kind="secondary"
                onPress={() => step.url && openInBrowser(store, step.url)}
              />
            ) : null}
            <Button
              testID="reset-cancel"
              title={t('common.cancel')}
              kind="secondary"
              onPress={cancel}
            />
          </>
        ) : (
          <>
            <View style={styles.done}>
              <StatusLine icon="check" color={palette.success} text={t('reset.done')} />
            </View>
            <Text style={[font.body, { color: palette.textMuted }]}>{t('reset.done_hint')}</Text>
            <Button
              testID="reset-set-up-recovery"
              title={t('recovery.set_up')}
              onPress={() => router.replace(routes.settingsRecoveryKey(id))}
            />
            <Button
              testID="reset-later"
              title={t('reset.later')}
              kind="secondary"
              onPress={() => router.back()}
            />
          </>
        )}
      </View>
      {prompt.element}
    </SettingsPage>
  );
}

/** One line of what a reset keeps or costs. */
function Consequence({ icon, color, text }: { icon: IconName; color: string; text: string }) {
  const palette = usePalette();

  return (
    <View style={styles.consequence}>
      <Icon name={icon} size={20} color={color} />
      <Text style={[font.body, { color: palette.text, flex: 1 }]}>{text}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { padding: 20, gap: 16 },
  consequence: { flexDirection: 'row', gap: 12, alignItems: 'flex-start' },
  busy: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  done: { marginHorizontal: -16 },
});
