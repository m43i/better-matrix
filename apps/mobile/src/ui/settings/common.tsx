// What the settings pages share: whose account a page shows, its heading, and the scrolling page.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams } from 'expo-router';
import type { ReactNode } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';

import { useStore } from '../../state/react';
import { account as accountOf, activeAccount } from '../../state/selectors';
import type { AccountData } from '../../state/types';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { EmptyState } from '../components';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';

/** The account a settings page shows: its `account` param, else the one on screen. */
export function useSettingsAccount(): AccountData | undefined {
  const { account } = useLocalSearchParams<{ account?: AccountId }>();

  return useStore((s) => (account ? accountOf(s, account) : activeAccount(s)));
}

/** A settings page: its native header title, then sections that scroll clear of the keyboard. */
export function SettingsPage({
  title,
  children,
  testID,
}: {
  title: string;
  children: ReactNode;
  testID?: string;
}) {
  const palette = usePalette();

  return (
    <>
      <Stack.Screen options={{ title }} />
      <KeyboardAwareScrollView
        testID={testID}
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.page}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        bottomOffset={24}
      >
        {children}
      </KeyboardAwareScrollView>
    </>
  );
}

/** A page for an account that is gone (signed out while it was open). */
export function NoAccount({ title }: { title: string }) {
  return (
    <>
      <Stack.Screen options={{ title }} />
      <EmptyState icon="user" title={title} />
    </>
  );
}

/** Whose account a page shows, with avatar, name, ID and homeserver. */
export function AccountHeading({ account }: { account: AccountData }) {
  const palette = usePalette();
  const { summary } = account;

  return (
    <View style={styles.heading}>
      <Avatar
        account={summary.id}
        id={summary.id}
        name={summary.display_name}
        media={summary.avatar}
        size={44}
      />
      <View style={styles.headingText}>
        <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
          {summary.display_name}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
          {`${summary.id} · ${summary.homeserver}`}
        </Text>
      </View>
    </View>
  );
}

/** A line of status: an icon in the status's colour and what it says. */
export function StatusLine({
  icon,
  color,
  text,
  testID,
}: {
  icon: IconName;
  color: string;
  text: string;
  testID?: string;
}) {
  const palette = usePalette();

  return (
    <View style={styles.status} testID={testID}>
      <Icon name={icon} size={22} color={color} />
      <Text style={[font.body, { color: palette.text, flex: 1 }]}>{text}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { paddingBottom: 48 },
  heading: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 20,
    paddingTop: 20,
  },
  headingText: { flex: 1, gap: 2 },
  status: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 14,
  },
});
