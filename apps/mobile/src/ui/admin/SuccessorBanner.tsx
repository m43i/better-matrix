// In place of the composer in an upgraded room: it was replaced, why (when the upgrade said), and
// the way to the new room, joined first when the user isn't in it yet.

import type { AccountId, Successor } from '@better-matrix/matrix-native/types';
import { useRouter } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { reachSuccessor } from '../../state/actions/admin';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useShown } from '../useShown';

export function SuccessorBanner({
  account,
  successor,
}: {
  account: AccountId;
  successor: Successor;
}) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const [busy, setBusy] = useState(false);
  const shown = useShown();

  const go = () => {
    if (busy) return;
    setBusy(true);
    reachSuccessor(store, account, successor).then(
      (roomId) => {
        // A join that ends after the user went elsewhere leaves them there.
        if (shown.current) router.replace(routes.room(account, roomId));
        else setBusy(false);
      },
      () => setBusy(false),
    );
  };

  return (
    <View
      testID="successor-banner"
      style={[styles.banner, { backgroundColor: palette.surface, borderColor: palette.divider }]}
    >
      <View style={styles.title}>
        <Icon name="upgrade" size={20} color={palette.accent} />
        <Text style={[font.bodyStrong, { color: palette.text }]}>{t('mobile.admin.replaced')}</Text>
      </View>
      <Text style={[font.meta, styles.body, { color: palette.textMuted }]}>
        {successor.reason ?? t('mobile.admin.replaced_body')}
      </Text>
      <Button
        testID="successor-go"
        title={t('mobile.admin.go_to_new_room')}
        busy={busy}
        onPress={go}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  banner: {
    borderTopWidth: StyleSheet.hairlineWidth,
    paddingHorizontal: 16,
    paddingVertical: 14,
    gap: 6,
  },
  title: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  body: { marginBottom: 6 },
});
