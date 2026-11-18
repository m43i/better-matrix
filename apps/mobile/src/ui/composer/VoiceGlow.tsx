// While a voice memo records, a soft blue glow around the composer's field that swells with the
// voice, as on the desktop: a faint halo always, and over it a brighter one whose opacity follows
// the microphone's level on the UI thread, so nothing re-renders for it. Under reduced motion the
// brighter halo holds still, half lit.

import { StyleSheet, View } from 'react-native';
import Animated, {
  type SharedValue,
  useAnimatedStyle,
  useReducedMotion,
  withTiming,
} from 'react-native-reanimated';

import { usePalette } from '../theme';

export function VoiceGlow({ level, radius }: { level: SharedValue<number>; radius: number }) {
  const palette = usePalette();
  const still = useReducedMotion();

  const voice = useAnimatedStyle(() => ({
    opacity: still ? 0.5 : withTiming(level.value, { duration: 90 }),
  }));

  return (
    <View pointerEvents="none" style={StyleSheet.absoluteFill} testID="composer-voice-glow">
      <View
        style={[
          StyleSheet.absoluteFill,
          { borderRadius: radius, boxShadow: `0 0 8px 1px ${palette.accent}59` },
        ]}
      />
      <Animated.View
        style={[
          StyleSheet.absoluteFill,
          { borderRadius: radius, boxShadow: `0 0 18px 4px ${palette.accent}cc` },
          voice,
        ]}
      />
    </View>
  );
}
