// The tab bar, the platform's own (UITabBar on iOS, Material's bottom navigation on Android):
// Home, Direct messages, and the account on screen. The account tab opens the account menu
// (switch or add an account, settings, sign out) instead of a screen: it is `disabled`, which
// keeps the bar from selecting it but still reports the press. Its label is the account's name;
// on iOS its icon is the account's picture when it has one (drawn as is), elsewhere the system's
// profile icon, since Android's bar tints every icon to one colour. Notifications are behind the
// bell in Home's header, creating things behind its "+".

import { router } from 'expo-router';
import { NativeTabs } from 'expo-router/native-tabs';
import { Platform } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { useStore } from '../../state/react';
import { hasUnread } from '../../state/sections';
import { activeAccount } from '../../state/selectors';
import { AVATAR_SIZE, useMedia } from '../../ui/media';
import { usePalette } from '../../ui/theme';

export default function TabsLayout() {
  const palette = usePalette();
  const summary = useStore((s) => activeAccount(s)?.summary);
  // Another account has something new: a dot on the account tab, as the desktop's rail shows.
  const othersUnread = useStore((s) => s.accounts.some((a) => a.summary.id !== s.active && hasUnread(a, 'home')));
  const picture = useMedia(summary?.id ?? null, Platform.OS === 'ios' ? summary?.avatar : null, AVATAR_SIZE);
  const name = summary ? summary.display_name || summary.id : t('mobile.tabs.you');
  return (
    <NativeTabs
      backgroundColor={palette.bg}
      indicatorColor={palette.sidebarSelected}
      iconColor={{ default: palette.textMuted, selected: palette.accent }}
      labelStyle={{ default: { color: palette.textMuted }, selected: { color: palette.accent } }}
      badgeBackgroundColor={palette.mention}
      labelVisibilityMode="labeled"
    >
      <NativeTabs.Trigger name="(home)">
        <NativeTabs.Trigger.Label>{t('mobile.tabs.home')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: 'house', selected: 'house.fill' }} md="home" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="dms">
        <NativeTabs.Trigger.Label>{t('mobile.tabs.dms')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon
          sf={{ default: 'bubble.left.and.bubble.right', selected: 'bubble.left.and.bubble.right.fill' }}
          md="forum"
        />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger
        name="you"
        disabled
        accessibilityLabel={t('mobile.shell.account_menu', { name })}
        listeners={{ tabPress: () => router.push(routes.accountMenu()) }}
      >
        <NativeTabs.Trigger.Label>{name}</NativeTabs.Trigger.Label>
        {picture ? (
          <NativeTabs.Trigger.Icon src={{ uri: picture }} renderingMode="original" />
        ) : (
          <NativeTabs.Trigger.Icon sf="person.crop.circle" md="account_circle" />
        )}
        {/* A badge without text is a dot (patches/expo-router: Material's small dot on Android). */}
        <NativeTabs.Trigger.Badge hidden={!othersUnread} />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
