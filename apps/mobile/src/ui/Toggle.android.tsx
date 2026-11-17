// An on/off switch on Android: Material 3's (Jetpack Compose), coloured from the accent as the
// other Compose controls are. React Native's own Android switch is Material 2's.

import { Host, Switch } from '@expo/ui/jetpack-compose';
import { View } from 'react-native';

import { usePalette } from './theme';
import type { ToggleProps } from './Toggle';

export function Toggle({ value, onChange, label }: ToggleProps) {
  const palette = usePalette();

  return (
    <View accessibilityLabel={label}>
      <Host matchContents colorScheme={palette.dark ? 'dark' : 'light'} seedColor={palette.accent}>
        <Switch
          value={value}
          onCheckedChange={onChange}
          colors={{ checkedTrackColor: palette.accent, checkedThumbColor: palette.onAccent }}
        />
      </Host>
    </View>
  );
}
