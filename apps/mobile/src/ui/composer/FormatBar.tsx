// The formatting toolbar Aa opens, in place of the composer's tools: every `Format`, grouped as
// on the desktop, the block formats on the caret's line shown as on.

import { ScrollView, StyleSheet, View } from 'react-native';

import { type Key, t } from '../../i18n';
import { FORMATS, type Format } from '../../state/format';
import { IconButton } from '../components';
import type { IconName } from '../icons';
import { usePalette } from '../theme';

const ITEMS: Record<Format, { icon: IconName; label: Key }> = {
  bold: { icon: 'bold', label: 'format.bold' },
  italic: { icon: 'italic', label: 'format.italic' },
  strike: { icon: 'strike', label: 'format.strike' },
  code: { icon: 'code', label: 'format.code' },
  link: { icon: 'link', label: 'format.link' },
  bullets: { icon: 'bullets', label: 'format.bullets' },
  numbers: { icon: 'numbers', label: 'format.numbers' },
  quote: { icon: 'quote', label: 'format.quote' },
  codeBlock: { icon: 'codeBlock', label: 'format.code_block' },
};

/** Where a new group starts in the toolbar (the desktop's `Format::starts_group`). */
const GROUP_STARTS: ReadonlySet<Format> = new Set(['link', 'bullets', 'codeBlock']);

export function FormatBar({
  active,
  onFormat,
}: {
  active: readonly Format[];
  onFormat: (format: Format) => void;
}) {
  const palette = usePalette();

  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      keyboardShouldPersistTaps="always"
      contentContainerStyle={styles.bar}
    >
      {FORMATS.map((format) => {
        const on = active.includes(format);

        return (
          <View
            key={format}
            style={[styles.item, GROUP_STARTS.has(format) ? styles.groupStart : null]}
          >
            <IconButton
              icon={ITEMS[format].icon}
              label={t(ITEMS[format].label)}
              onPress={() => onFormat(format)}
              size={36}
              color={on ? palette.accent : palette.text}
              background={on ? palette.hover : undefined}
              testID={`composer-format-${format}`}
            />
          </View>
        );
      })}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  bar: { alignItems: 'center', gap: 2 },
  item: {},
  groupStart: { marginLeft: 8 },
});
