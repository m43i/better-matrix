// What a QR sign-in shows at each step, on either device (`QrLoginFlow`): the scanner or this
// device's code, the check code to type or to show, the approval, and how it ended. The screens
// around it (signing in, linking a device) add their own heading and ways out.

import { useState, useSyncExternalStore } from 'react';
import { ActivityIndicator, StyleSheet, Text, useWindowDimensions, View } from 'react-native';

import { t } from '../../i18n';
import { formatCheckCode, type QrLoginFlow } from '../../state/actions/qrLogin';
import { font } from '../../theme/palette';
import { Button, TextField } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { QrCode } from './QrCode';
import { QrScanner } from './QrScanner';

export function QrLoginPanel({
  flow,
  scanHint,
  showHint,
  doneText,
  onRetry,
  onDone,
}: {
  flow: QrLoginFlow;
  /** Whose code to scan. */
  scanHint: string;
  /** What to do with this device's code on the other one. */
  showHint: string;
  /** What the done step says. */
  doneText: string;
  /** Starts again after a failure. */
  onRetry: () => void;
  /** Leaves once it's done, when the screen doesn't by itself. */
  onDone?: () => void;
}) {
  const palette = usePalette();
  const view = useSyncExternalStore(flow.subscribe, flow.snapshot, flow.snapshot);
  const { width } = useWindowDimensions();
  const [digits, setDigits] = useState('');

  const muted = (text: string, testID?: string) => (
    <Text testID={testID} style={[font.body, styles.center, { color: palette.textMuted }]}>
      {text}
    </Text>
  );

  switch (view.type) {
    case 'scanning':
      return (
        <View style={styles.panel}>
          {muted(scanHint)}
          <QrScanner testID="qr-scanner" onCode={(code) => flow.scanned(code)} />
        </View>
      );
    case 'starting':
    case 'syncing':
      return (
        <View style={styles.panel} testID={`qr-login-${view.type}`}>
          <ActivityIndicator color={palette.textMuted} />
          {muted(
            view.type === 'starting'
              ? t('mobile.accounts.qr_starting')
              : t('mobile.accounts.qr_syncing'),
          )}
        </View>
      );
    case 'show':
      return (
        <View style={styles.panel}>
          <QrCode testID="qr-login-code" qr={view.qr} size={Math.min(width - 64, 320)} />
          {muted(showHint)}
        </View>
      );
    case 'enterCheckCode': {
      const submit = () => flow.enterCheckCode(digits);

      return (
        <View style={styles.panel}>
          {muted(t('mobile.accounts.qr_enter_check_code'))}
          <TextField
            testID="qr-check-code"
            label={t('mobile.accounts.check_code')}
            value={digits}
            onChangeText={(text) => setDigits(text.replace(/\D/g, '').slice(0, 2))}
            keyboardType="number-pad"
            maxLength={2}
            autoFocus
            returnKeyType="done"
            onSubmitEditing={submit}
            style={styles.digitsField}
          />
          <Button
            testID="qr-check-code-continue"
            title={t('common.continue')}
            busy={view.sent}
            disabled={digits.length !== 2}
            onPress={submit}
          />
        </View>
      );
    }

    case 'checkCode':
      return (
        <View style={styles.panel}>
          {muted(t('mobile.accounts.qr_check_code'))}
          <Text
            testID="qr-shown-check-code"
            style={[styles.digits, { color: palette.text }]}
            selectable
          >
            {formatCheckCode(view.code)}
          </Text>
        </View>
      );
    case 'approve':
      return (
        <View style={styles.panel}>
          <Icon name="external" size={36} color={palette.accent} />
          {muted(t('mobile.accounts.qr_approve'))}
          <Button
            testID="qr-open-again"
            title={t('mobile.accounts.qr_open_again')}
            kind="secondary"
            onPress={() => flow.reopen()}
          />
        </View>
      );
    case 'waiting':
      return (
        <View style={styles.panel} testID="qr-login-waiting">
          <ActivityIndicator color={palette.textMuted} />
          {muted(
            view.code === null
              ? t('mobile.accounts.qr_waiting')
              : t('mobile.accounts.qr_waiting_code', { code: view.code }),
          )}
        </View>
      );
    case 'finishing':
      // Being saved: the screen leaves (or offers to) once the command finished.
      return (
        <View style={styles.panel} testID="qr-login-finishing">
          <ActivityIndicator color={palette.textMuted} />
          {muted(doneText)}
        </View>
      );
    case 'done':
      return (
        <View style={styles.panel} testID="qr-login-done">
          <Icon name="verified" size={40} color={palette.success} />
          {muted(doneText)}
          {onDone ? (
            <Button testID="qr-login-close" title={t('common.done')} onPress={onDone} />
          ) : null}
        </View>
      );
    case 'failed':
      return (
        <View style={styles.panel}>
          <Icon name="alert" size={36} color={palette.warning} />
          <Text
            testID="qr-login-error"
            style={[font.body, styles.center, { color: palette.danger }]}
          >
            {view.error || t('mobile.accounts.qr_failed')}
          </Text>
          <Button testID="qr-try-again" title={t('common.try_again')} onPress={onRetry} />
        </View>
      );
  }
}

const styles = StyleSheet.create({
  panel: { alignItems: 'stretch', gap: 16 },
  center: { textAlign: 'center' },
  digits: { fontSize: 56, fontWeight: '700', letterSpacing: 8, textAlign: 'center' },
  digitsField: { fontSize: 28, textAlign: 'center', letterSpacing: 8 },
});
