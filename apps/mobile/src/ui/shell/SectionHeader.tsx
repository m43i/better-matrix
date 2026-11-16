// A section's header in the room list: a tap collapses or expands it, a long press opens its
// menu (rename and delete for the user's own sections).

import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { font } from '../../theme/palette';
import { Touchable } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function SectionHeader({
  id,
  title,
  collapsed,
  count,
  onToggle,
  onMenu,
}: {
  id: string;
  title: string;
  collapsed: boolean;
  /** Conversations wanting attention in it, shown while it is collapsed. */
  count: number;
  onToggle: () => void;
  onMenu?: () => void;
}) {
  const palette = usePalette();

  return (
    <View style={{ backgroundColor: palette.bg }}>
      <Touchable
        testID={`section-${id}`}
        accessibilityRole="button"
        accessibilityState={{ expanded: !collapsed }}
        accessibilityLabel={title}
        accessibilityHint={collapsed ? t('sections.expand') : t('sections.collapse')}
        onPress={onToggle}
        onLongPress={onMenu}
        delayLongPress={350}
        style={styles.header}
      >
        <Icon
          name={collapsed ? 'chevronRight' : 'chevronDown'}
          size={14}
          color={palette.textMuted}
        />
        <Text style={[font.section, styles.title, { color: palette.textMuted }]} numberOfLines={1}>
          {title}
        </Text>
        {collapsed && count > 0 ? (
          <View style={[styles.dot, { backgroundColor: palette.accent }]} />
        ) : null}
      </Touchable>
    </View>
  );
}

const styles = StyleSheet.create({
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingHorizontal: 18,
    paddingTop: 16,
    paddingBottom: 6,
  },
  title: { flex: 1 },
  dot: { width: 8, height: 8, borderRadius: 4 },
});
