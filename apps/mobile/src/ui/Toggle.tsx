// An on/off switch, the platform's own: UISwitch on iOS (Toggle.android.tsx draws Material 3's).

import { Switch } from 'react-native';

import { usePalette } from './theme';

export interface ToggleProps {
  value: boolean;
  onChange: (value: boolean) => void;
  /** What it switches, for screen readers. */
  label: string;
}

export function Toggle({ value, onChange, label }: ToggleProps) {
  const palette = usePalette();

  return (
    <Switch
      value={value}
      onValueChange={onChange}
      trackColor={{ true: palette.accent, false: palette.divider }}
      accessibilityLabel={label}
    />
  );
}
