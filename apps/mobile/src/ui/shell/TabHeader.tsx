// A tab's header: its title, and the account on screen at the right, its picture opening the
// account menu (switch or add an account, settings, sign out), with a dot when another account
// has something new, as the desktop's rail shows. iOS places the picture as a toolbar item
// without the shared glass background. Android's native stack lays a custom title out across the
// whole bar, which pushes a right item to the bar's start, so there the title and the picture
// share one row, the picture at its end.

import { router, Stack } from 'expo-router';
import type { ReactNode } from 'react';
import { Platform, Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { useStore } from '../../state/react';
import { hasUnread } from '../../state/sections';
import { activeAccount } from '../../state/selectors';
import { font } from '../../theme/palette';
import { usePalette } from '../theme';
import { AccountAvatar, WithDot } from './AccountButton';

const SIZE = 32;

function ProfileButton() {
  const summary = useStore((s) => activeAccount(s)?.summary);

  const othersUnread = useStore((s) =>
    s.accounts.some((a) => a.summary.id !== s.active && hasUnread(a, 'home')),
  );

  if (!summary) return null;
  const name = summary.display_name || summary.id;

  return (
    <Pressable
      testID="profile-button"
      accessibilityRole="button"
      accessibilityLabel={t('mobile.shell.account_menu', { name })}
      hitSlop={8}
      onPress={() => router.push(routes.accountMenu())}
      style={({ pressed }) => ({ opacity: pressed ? 0.6 : 1 })}
    >
      <WithDot on={othersUnread} size={SIZE}>
        <AccountAvatar summary={summary} size={SIZE} />
      </WithDot>
    </Pressable>
  );
}

/** The title drawn as the header draws a plain one, for Android's row. */
function PlainTitle({ title }: { title: string }) {
  const palette = usePalette();

  return (
    <Text style={[font.workspace, styles.plain, { color: palette.text }]} numberOfLines={1}>
      {title}
    </Text>
  );
}

/** A tab's header: `title` (or `titleView` in its place) and the account's picture at the right. */
export function TabHeader({ title, titleView }: { title: string; titleView?: ReactNode }) {
  if (Platform.OS === 'android') {
    return (
      <Stack.Screen
        options={{
          title,
          headerTitle: () => (
            <View style={styles.row}>
              <View style={styles.title}>{titleView ?? <PlainTitle title={title} />}</View>
              <ProfileButton />
            </View>
          ),
        }}
      />
    );
  }

  return (
    <>
      <Stack.Screen
        options={titleView === undefined ? { title } : { title, headerTitle: () => titleView }}
      />
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.View hidesSharedBackground>
          <ProfileButton />
        </Stack.Toolbar.View>
      </Stack.Toolbar>
    </>
  );
}

const styles = StyleSheet.create({
  row: { flex: 1, flexDirection: 'row', alignItems: 'center', paddingRight: 16, gap: 12 },
  title: { flex: 1, flexDirection: 'row' },
  plain: { fontSize: 20 },
});
