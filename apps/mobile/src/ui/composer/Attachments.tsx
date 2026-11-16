// Files waiting to be sent, above the field: a thumbnail for pictures, an icon and name for
// anything else, each with a button that takes it out.

import { Image } from 'expo-image';
import { ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import type { PendingAttachment } from '../../state/composer';
import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

const SIZE = 64;

export function Attachments({ attachments, onRemove }: { attachments: PendingAttachment[]; onRemove: (path: string) => void }) {
  const palette = usePalette();
  if (attachments.length === 0) return null;
  return (
    <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.list} keyboardShouldPersistTaps="handled">
      {attachments.map((attachment) => (
        <View key={attachment.path} style={styles.item} testID={`composer-attachment-${attachment.name}`}>
          {attachment.kind === 'image' ? (
            <Image source={{ uri: attachment.uri }} style={[styles.thumb, { backgroundColor: palette.surface }]} contentFit="cover" />
          ) : (
            <View style={[styles.file, { backgroundColor: palette.surface, borderColor: palette.divider }]}>
              <Icon name={attachment.kind === 'video' ? 'video' : 'file'} size={22} color={palette.textMuted} />
              <Text style={[font.meta, styles.name, { color: palette.text }]} numberOfLines={2}>
                {attachment.name}
              </Text>
            </View>
          )}
          <View style={styles.remove}>
            <IconButton
              icon="close"
              label={t('composer.remove', { name: attachment.name })}
              onPress={() => onRemove(attachment.path)}
              size={22}
              color={palette.onAccent}
              background={palette.scrim}
              testID={`composer-attachment-remove-${attachment.name}`}
            />
          </View>
        </View>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  list: { gap: 10, paddingHorizontal: 12, paddingTop: 10 },
  item: { width: SIZE, height: SIZE },
  thumb: { width: SIZE, height: SIZE, borderRadius: 10 },
  file: {
    width: SIZE,
    height: SIZE,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
    padding: 4,
  },
  name: { fontSize: 10, textAlign: 'center' },
  remove: { position: 'absolute', top: -6, right: -6 },
});
