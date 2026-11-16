// "This session isn't verified" with a way to verify, for the account on screen, as the desktop's
// banner. Owned by the accounts area; shown at the top of Home.

import { router } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { unverifiedAccount } from '../../state/actions/security';
import { useStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function SecurityBanner() {
  const palette = usePalette();
  const account = useStore(unverifiedAccount);

  if (account === null) return null;

  return (
    <View
      testID="security-banner"
      style={[
        styles.banner,
        { backgroundColor: `${palette.warning}22`, borderColor: `${palette.warning}66` },
      ]}
    >
      <Icon name="lock" size={20} color={palette.warning} />
      <Text style={[font.body, styles.text, { color: palette.text }]}>{t('account.banner')}</Text>
      <Button
        testID="security-banner-verify"
        title={t('account.banner_action')}
        kind="plain"
        onPress={() => router.push(routes.verify(account))}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  banner: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    marginHorizontal: 16,
    marginVertical: 8,
    paddingLeft: 14,
    borderRadius: 12,
    borderWidth: StyleSheet.hairlineWidth,
  },
  text: { flex: 1, paddingVertical: 12 },
});
