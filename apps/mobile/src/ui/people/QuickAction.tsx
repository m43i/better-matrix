// One of the round buttons under a room's name in its details, with its label below, as Slack's
// channel details lay them out.

import { Platform, Pressable, StyleSheet, Text, View } from 'react-native';

import { font } from '../../theme/palette';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';

export function QuickAction({
  icon,
  label,
  onPress,
  danger = false,
  active = false,
  testID,
}: {
  icon: IconName;
  label: string;
  onPress: () => void;
  danger?: boolean;
  /** Whether what it opens is open. */
  active?: boolean;
  testID: string;
}) {
  const palette = usePalette();
  const color = danger ? palette.danger : active ? palette.accent : palette.text;
  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ expanded: active }}
      android_ripple={{ color: palette.hover, borderless: true, radius: 36 }}
      style={({ pressed }) => [styles.action, { opacity: pressed && Platform.OS === 'ios' ? 0.6 : 1 }]}
    >
      <View style={[styles.circle, { backgroundColor: palette.surface, borderColor: active ? palette.accent : palette.divider }]}>
        <Icon name={icon} size={22} color={color} />
      </View>
      <Text style={[font.meta, { color }]} numberOfLines={1}>
        {label}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  action: { alignItems: 'center', gap: 6, width: 76 },
  circle: {
    width: 48,
    height: 48,
    borderRadius: 24,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: StyleSheet.hairlineWidth,
  },
});
