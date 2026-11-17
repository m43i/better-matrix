// The tab bar, the platform's own (UITabBar on iOS, Material's bottom navigation on Android):
// Home and Direct messages. The account is the avatar at the right of each tab's header
// (`ui/shell/TabHeader`), and the search sits under each tab's title.
//
// On iOS the bar also holds "+", which opens the create sheet instead of a screen: it is
// `disabled`, which keeps the bar from selecting it but still reports the press. iOS 27 sets it
// on its own surface beside the tabs (`prominent`), iOS 26.1 in the search tab's separated place
// (`role="search"`, its search never activated), and older iOS shows it as a third tab. Android
// has no such tab: a floating button over each tab's list opens the same menu (`CreateFab`).

import { router } from 'expo-router';
import { NativeTabs } from 'expo-router/native-tabs';
import { Platform } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { createTab } from '../../ui/shell/create';
import { usePalette } from '../../ui/theme';

/** Fixed for the app's run: a tab's `role` must not change after it renders. */
const CREATE_TAB = createTab(Platform.OS, Platform.Version);

export default function TabsLayout() {
  const palette = usePalette();

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
          sf={{
            default: 'bubble.left.and.bubble.right',
            selected: 'bubble.left.and.bubble.right.fill',
          }}
          md="forum"
        />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger
        name="new"
        hidden={CREATE_TAB === 'none'}
        disabled
        role={CREATE_TAB === 'separate' ? 'search' : undefined}
        disableAutomaticSearchActivation={CREATE_TAB === 'separate' ? true : undefined}
        prominent={CREATE_TAB === 'prominent'}
        testID="tab-create"
        accessibilityLabel={t('create.menu')}
        listeners={{ tabPress: () => router.push(routes.create()) }}
      >
        <NativeTabs.Trigger.Label>{t('create.menu')}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="plus" md="add" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
