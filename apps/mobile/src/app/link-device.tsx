// Linking a new device (modal, from Settings → Sessions): this verified OAuth session shows a QR
// code for the new device to scan, or scans the one it shows (`scan`), then the two agree on a
// check code and the account page approves the new sign-in. Leaving stops it.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { router, Stack, useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';

import { t } from '../i18n';
import { linkDevice, QrLoginFlow } from '../state/actions/qrLogin';
import { useAppStore } from '../state/react';
import { openInBrowser } from '../ui/accounts/browser';
import { HeaderCancel } from '../ui/HeaderButtons';
import { QrLoginPanel } from '../ui/qr/QrLoginPanel';
import { usePalette } from '../ui/theme';

export default function LinkDeviceScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const { account, scan } = useLocalSearchParams<{ account: AccountId; scan?: string }>();
  const scanning = scan === '1';

  const [flow] = useState(
    () => new QrLoginFlow(store, linkDevice(account), (url) => openInBrowser(store, url)),
  );

  const start = () => (scanning ? flow.scan() : flow.show());

  useEffect(() => {
    if (scanning) flow.scan();
    else flow.show();

    return () => flow.cancel();
  }, [flow, scanning]);

  return (
    <>
      <Stack.Screen
        options={{
          title: t('mobile.accounts.link_device'),
          headerShown: true,
          headerLeft: () => (
            <HeaderCancel onPress={() => router.back()} testID="link-device-cancel" />
          ),
        }}
      />
      <ScrollView
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.page}
        keyboardShouldPersistTaps="handled"
        testID="link-device"
      >
        <QrLoginPanel
          flow={flow}
          scanHint={t('mobile.accounts.link_scan_hint')}
          showHint={t('mobile.accounts.link_show_hint')}
          doneText={t('mobile.accounts.link_done')}
          onRetry={start}
          onDone={() => router.back()}
        />
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  page: { padding: 24, gap: 18 },
});
