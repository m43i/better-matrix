// The settings pages' native stack. Its first page opens over the tabs from the root stack, where
// this stack has nothing to go back to: that page gets a back button that closes the settings.

import { router, Stack } from 'expo-router';

import { t } from '../../i18n';
import { IconButton } from '../../ui/components';
import { headerTint, usePalette } from '../../ui/theme';

export default function SettingsLayout() {
  const palette = usePalette();

  return (
    <Stack
      screenOptions={({ navigation, route }) => ({
        headerTintColor: headerTint(palette),
        headerTitleStyle: { color: palette.text },
        headerStyle: { backgroundColor: palette.bg },
        contentStyle: { backgroundColor: palette.bg },
        headerBackButtonDisplayMode: 'minimal',
        headerLeft:
          navigation.getState()?.routes[0]?.key === route.key
            ? () => (
                <IconButton
                  testID="settings-back"
                  icon="back"
                  label={t('mobile.accounts.back')}
                  color={headerTint(palette)}
                  size={36}
                  onPress={() => router.back()}
                />
              )
            : undefined,
      })}
    />
  );
}
