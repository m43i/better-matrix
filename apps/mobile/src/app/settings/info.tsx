// Info: the app's name and version, and the open-source software it is built from.

import Constants from 'expo-constants';
import { Fragment, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { font } from '../../theme/palette';
import { Logo } from '../../ui/accounts/Logo';
import { Divider, ListRow, ListSection } from '../../ui/components';
import { SettingsPage } from '../../ui/settings/common';
import { THIRD_PARTY } from '../../ui/settings/thirdParty';
import { usePalette } from '../../ui/theme';

export default function InfoScreen() {
  const palette = usePalette();
  const [shown, setShown] = useState(false);
  const version = Constants.expoConfig?.version ?? Constants.nativeApplicationVersion ?? '';
  return (
    <SettingsPage title={t('settings.info')} testID="info">
      <View style={styles.app}>
        <Logo size={64} color={palette.accent} />
        <Text style={[font.workspace, { color: palette.text, fontSize: 22 }]}>{t('app.name')}</Text>
        <Text testID="app-version" style={[font.body, { color: palette.textMuted }]}>
          {t('info.version', { version })}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]}>{t('mobile.accounts.license')}</Text>
      </View>
      <ListSection title={t('info.third_party')} footer={t('mobile.accounts.third_party_hint')}>
        <ListRow
          testID="third-party-toggle"
          icon="package"
          title={shown ? t('info.hide_third_party') : t('info.show_third_party', { count: THIRD_PARTY.length })}
          chevron={false}
          onPress={() => setShown((s) => !s)}
        />
        {shown
          ? THIRD_PARTY.map((component) => (
              <Fragment key={component.name}>
                <Divider inset={16} />
                <ListRow
                  testID={`component-${component.name}`}
                  title={component.name}
                  subtitle={component.version}
                  value={component.license}
                />
              </Fragment>
            ))
          : null}
      </ListSection>
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  app: { alignItems: 'center', gap: 8, paddingTop: 28 },
});
