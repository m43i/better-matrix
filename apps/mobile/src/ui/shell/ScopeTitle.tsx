// Home's title: the scope on screen (Home, or a space's name) with a chevron, opening the space
// picker, as the desktop's rail switches scopes.

import { router } from 'expo-router';
import { Pressable, StyleSheet, Text } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function ScopeTitle({ title, unread }: { title: string; unread: boolean }) {
  const palette = usePalette();
  return (
    <Pressable
      testID="scope-title"
      accessibilityRole="button"
      accessibilityLabel={title}
      accessibilityHint={t('mobile.shell.switch_space')}
      hitSlop={8}
      onPress={() => router.push(routes.spaces())}
      style={({ pressed }) => [styles.title, { opacity: pressed ? 0.6 : 1 }]}
    >
      <Text style={[font.workspace, styles.text, { color: palette.text }]} numberOfLines={1}>
        {title}
      </Text>
      <Icon name="chevronDown" size={16} color={palette.textMuted} weight="semibold" />
      {unread ? <Text style={[styles.dot, { backgroundColor: palette.mention }]} /> : null}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  title: { flexDirection: 'row', alignItems: 'center', gap: 6, maxWidth: 260 },
  text: { flexShrink: 1, fontSize: 20 },
  dot: { width: 8, height: 8, borderRadius: 4, overflow: 'hidden' },
});
