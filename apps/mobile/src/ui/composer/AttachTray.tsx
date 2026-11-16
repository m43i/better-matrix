// What + opens below the composer, in the keyboard's place, as Slack's does: the photo
// library, the camera, files, and a poll.

import { Pressable, StyleSheet, Text, View } from 'react-native';

import { type Key, t } from '../../i18n';
import { font } from '../../theme/palette';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';

export type TrayChoice = 'library' | 'camera' | 'files' | 'poll';

const CHOICES: ReadonlyArray<{ id: TrayChoice; icon: IconName; label: Key }> = [
  { id: 'library', icon: 'library', label: 'mobile.composer.library' },
  { id: 'camera', icon: 'camera', label: 'mobile.composer.camera' },
  { id: 'files', icon: 'folder', label: 'mobile.composer.files' },
  { id: 'poll', icon: 'poll', label: 'mobile.composer.poll' },
];

export function AttachTray({ onChoose }: { onChoose: (choice: TrayChoice) => void }) {
  const palette = usePalette();
  return (
    <View style={styles.tray}>
      {CHOICES.map((choice) => (
        <Pressable
          key={choice.id}
          testID={`composer-tray-${choice.id}`}
          accessibilityRole="button"
          accessibilityLabel={t(choice.label)}
          onPress={() => onChoose(choice.id)}
          android_ripple={{ color: palette.hover, borderless: true }}
          style={({ pressed }) => [styles.choice, { opacity: pressed ? 0.6 : 1 }]}
        >
          <View style={[styles.circle, { backgroundColor: palette.surface, borderColor: palette.divider }]}>
            <Icon name={choice.icon} size={24} color={palette.text} />
          </View>
          <Text style={[font.meta, { color: palette.text, textAlign: 'center' }]} numberOfLines={2}>
            {t(choice.label)}
          </Text>
        </Pressable>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  tray: { flexDirection: 'row', justifyContent: 'space-around', paddingHorizontal: 12, paddingVertical: 16 },
  choice: { width: 76, alignItems: 'center', gap: 6 },
  circle: {
    width: 56,
    height: 56,
    borderRadius: 28,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: StyleSheet.hairlineWidth,
  },
});
