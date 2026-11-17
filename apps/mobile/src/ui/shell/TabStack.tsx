// The native stack inside each tab: what gives a tab its own header (title, header buttons,
// search) under the native tab bar. Styled as the root stack is.

import { Stack } from 'expo-router';

import { headerTint, usePalette } from '../theme';

export function TabStack() {
  const palette = usePalette();

  return (
    <Stack
      screenOptions={{
        headerTintColor: headerTint(palette),
        headerTitleStyle: { color: palette.text },
        headerStyle: { backgroundColor: palette.bg },
        contentStyle: { backgroundColor: palette.bg },
        headerShadowVisible: false,
        headerBackButtonDisplayMode: 'minimal',
      }}
    />
  );
}
