// The line above the field while replying ("Replying to Ada" and a quote) or editing, with a
// button that drops it.

import { StyleSheet, Text, View } from 'react-native';

import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';

export function Banner({
  icon,
  title,
  quote,
  cancelLabel,
  onCancel,
  testID,
}: {
  icon: IconName;
  title: string;
  quote?: string;
  cancelLabel: string;
  onCancel: () => void;
  testID: string;
}) {
  const palette = usePalette();
  return (
    <View style={styles.banner} testID={testID}>
      <View style={[styles.bar, { backgroundColor: palette.accent }]} />
      <Icon name={icon} size={16} color={palette.textMuted} />
      <View style={styles.text}>
        <Text style={[font.section, { color: palette.text }]} numberOfLines={1}>
          {title}
        </Text>
        {quote ? (
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {quote}
          </Text>
        ) : null}
      </View>
      <IconButton icon="close" label={cancelLabel} onPress={onCancel} size={32} color={palette.textMuted} testID={`${testID}-cancel`} />
    </View>
  );
}

const styles = StyleSheet.create({
  banner: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingLeft: 12, paddingRight: 6, paddingTop: 8 },
  bar: { width: 3, alignSelf: 'stretch', borderRadius: 2 },
  text: { flex: 1, gap: 1 },
});
