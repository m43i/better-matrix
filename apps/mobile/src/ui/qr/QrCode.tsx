// A QR code the engine made (`QrImage`), drawn black on white with its quiet zone whatever the
// theme, since scanners need the contrast.

import type { QrImage } from '@better-matrix/matrix-native/types';
import { useMemo } from 'react';
import { PixelRatio, StyleSheet, View } from 'react-native';

import { t } from '../../i18n';
import { darkRuns, moduleSize, QUIET_ZONE } from './modules';

export function QrCode({ qr, size, testID }: { qr: QrImage; size: number; testID?: string }) {
  const runs = useMemo(() => darkRuns(qr), [qr]);
  const module = moduleSize(qr, size, PixelRatio.get());
  const side = module * (qr.width + 2 * QUIET_ZONE);

  return (
    <View
      testID={testID}
      accessible
      accessibilityRole="image"
      accessibilityLabel={t('mobile.accounts.qr_code_label')}
      style={[styles.code, { width: side, height: side }]}
    >
      {runs.map((run) => (
        <View
          key={`${run.y}-${run.x}`}
          style={[
            styles.dark,
            {
              left: (run.x + QUIET_ZONE) * module,
              top: (run.y + QUIET_ZONE) * module,
              width: run.length * module,
              height: module,
            },
          ]}
        />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  code: { backgroundColor: '#ffffff', borderRadius: 8, alignSelf: 'center' },
  dark: { position: 'absolute', backgroundColor: '#000000' },
});
