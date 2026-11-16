// "Jump to…": the search field at the top of Home, which opens the search screen.

import { router } from 'expo-router';
import { StyleSheet, Text } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Touchable } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function JumpTo() {
  const palette = usePalette();
  return (
    <Touchable
      testID="jump-to"
      accessibilityRole="search"
      accessibilityLabel={t('mobile.shell.jump_to')}
      onPress={() => router.push(routes.search())}
      style={{ ...styles.field, backgroundColor: palette.surface, borderColor: palette.divider }}
    >
      <Icon name="search" size={18} color={palette.textMuted} />
      <Text style={[font.body, { color: palette.textMuted }]}>{t('mobile.shell.jump_to')}</Text>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  field: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    marginHorizontal: 16,
    marginTop: 8,
    marginBottom: 4,
    paddingHorizontal: 14,
    height: 42,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
});
