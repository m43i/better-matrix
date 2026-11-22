// The one QR scanner (verification, signing in, linking a device). Matrix's codes carry binary
// data, which the camera's barcode scanner hands over as lossy text, so its sightings only say
// when to look: the scanner takes a small picture then (and every few seconds anyway) and the
// engine reads the code's bytes from it (`ReadQrCode`, ADR 0004). Demo scenes, which have no
// camera to point, show a placeholder that reads the demo engine's code when tapped.

import { File } from 'expo-file-system';
import { CameraView, useCameraPermissions } from 'expo-camera';
import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { Linking, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { readQrCode } from '../../state/actions/security';
import { pathFromUri } from '../../state/outgoing';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button, Touchable } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

/** Whether scanners show a placeholder instead of the camera (demo scenes). */
export const ScannerPlaceholder = createContext(false);

/** How often the scanner looks without a sighting: binary codes don't always register as one. */
const LOOK_EVERY_MS = 2500;

/** The side of the square the camera shows. */
const SIDE = 280;

/**
 * Reads codes from pictures, one at a time, until one reads: `look(take)` takes a picture with
 * `take` and hands its code to `onCode` once.
 */
function useQrReader(onCode: (code: number[]) => void) {
  const store = useAppStore();
  const busy = useRef(false);
  const done = useRef(false);
  const latest = useRef(onCode);
  useEffect(() => {
    latest.current = onCode;
  }, [onCode]);

  return useCallback(
    async (take: () => Promise<string | null>) => {
      if (busy.current || done.current) return;
      busy.current = true;

      try {
        const uri = await take();

        if (uri === null) return;
        const code = await readQrCode(store, pathFromUri(uri)).catch(() => null);

        try {
          new File(uri).delete();
        } catch {
          // The cache is cleaned by the system too.
        }

        if (code !== null && !done.current) {
          done.current = true;
          latest.current(code);
        }
      } catch {
        // The camera was busy or closing: the next sighting looks again.
      } finally {
        busy.current = false;
      }
    },
    [store],
  );
}

export function QrScanner({
  onCode,
  testID,
}: {
  onCode: (code: number[]) => void;
  testID: string;
}) {
  const placeholder = useContext(ScannerPlaceholder);
  const look = useQrReader(onCode);

  return (
    <View style={styles.scanner} testID={testID}>
      {placeholder ? (
        <Placeholder onPress={() => void look(() => Promise.resolve('demo.jpg'))} />
      ) : (
        <Camera look={look} />
      )}
    </View>
  );
}

function Camera({ look }: { look: (take: () => Promise<string | null>) => Promise<void> }) {
  const palette = usePalette();
  const [permission, requestPermission] = useCameraPermissions();
  const camera = useRef<CameraView>(null);
  const [ready, setReady] = useState(false);

  const capture = useCallback(() => {
    const view = camera.current;

    if (!view || !ready) return;
    void look(async () => {
      const picture = await view.takePictureAsync({ quality: 0.5, shutterSound: false });

      return picture.uri;
    });
  }, [look, ready]);

  useEffect(() => {
    if (!ready) return;
    const timer = setInterval(capture, LOOK_EVERY_MS);

    return () => clearInterval(timer);
  }, [ready, capture]);

  if (!permission) return null;

  if (!permission.granted) {
    return (
      <View style={[styles.square, styles.ask, { backgroundColor: palette.surface }]}>
        <Icon name="camera" size={32} color={palette.textMuted} />
        <Text style={[font.body, styles.center, { color: palette.text }]}>
          {permission.canAskAgain
            ? t('mobile.accounts.qr_camera_needed')
            : t('mobile.accounts.qr_camera_denied')}
        </Text>
        {permission.canAskAgain ? (
          <Button
            testID="qr-allow-camera"
            title={t('mobile.accounts.qr_allow_camera')}
            onPress={() => void requestPermission()}
          />
        ) : (
          <Button
            testID="qr-open-settings"
            title={t('mobile.accounts.qr_open_settings')}
            kind="secondary"
            onPress={() => void Linking.openSettings()}
          />
        )}
      </View>
    );
  }

  return (
    <>
      <View style={styles.square}>
        <CameraView
          ref={camera}
          style={StyleSheet.absoluteFill}
          facing="back"
          barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
          onBarcodeScanned={capture}
          onCameraReady={() => setReady(true)}
        />
        <View style={[styles.frame, { borderColor: palette.onAccent }]} pointerEvents="none" />
      </View>
      <Text style={[font.body, styles.center, { color: palette.textMuted }]}>
        {t('mobile.accounts.qr_scan_hint')}
      </Text>
    </>
  );
}

function Placeholder({ onPress }: { onPress: () => void }) {
  const palette = usePalette();

  return (
    <>
      <Touchable
        testID="qr-placeholder"
        accessibilityRole="button"
        accessibilityLabel={t('mobile.accounts.qr_scan_hint')}
        onPress={onPress}
        style={{ ...styles.square, ...styles.ask, backgroundColor: '#111111' }}
      >
        <Icon name="qrScan" size={64} color="#ffffff" />
      </Touchable>
      <Text style={[font.body, styles.center, { color: palette.textMuted }]}>
        {t('mobile.accounts.qr_scan_hint')}
      </Text>
    </>
  );
}

const styles = StyleSheet.create({
  scanner: { alignItems: 'center', gap: 12 },
  square: { width: SIDE, height: SIDE, borderRadius: 16, overflow: 'hidden' },
  ask: { alignItems: 'center', justifyContent: 'center', gap: 14, padding: 20 },
  frame: { position: 'absolute', inset: 40, borderWidth: 3, borderRadius: 12, opacity: 0.85 },
  center: { textAlign: 'center' },
});
