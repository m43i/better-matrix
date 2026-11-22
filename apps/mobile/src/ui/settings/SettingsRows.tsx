// The way to every settings page, as the You tab and the Settings page list it: the account's
// pages (for the account on screen), then this device's preferences and the app's info.

import { router } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { useStore } from '../../state/react';
import { Divider, ListRow, ListSection } from '../components';

export function SettingsRows() {
  const active = useStore((s) => s.active);

  return (
    <>
      {active ? (
        <ListSection>
          <ListRow
            testID="open-profile"
            icon="user"
            title={t('settings.profile')}
            onPress={() => router.push(routes.settingsProfile(active))}
          />
          <Divider inset={52} />
          <ListRow
            testID="open-notifications"
            icon="bell"
            title={t('settings.notifications')}
            onPress={() => router.push(routes.settingsNotifications())}
          />
          <Divider inset={52} />
          <ListRow
            testID="open-account"
            icon="shield"
            title={t('mobile.accounts.security_title')}
            onPress={() => router.push(routes.settingsAccount(active))}
          />
          <Divider inset={52} />
          <ListRow
            testID="open-safety"
            icon="safety"
            title={t('settings.safety')}
            onPress={() => router.push(routes.settingsSafety(active))}
          />
          <Divider inset={52} />
          <ListRow
            testID="open-sessions"
            icon="devices"
            title={t('settings.sessions')}
            onPress={() => router.push(routes.settingsSessions(active))}
          />
        </ListSection>
      ) : null}
      <ListSection>
        <ListRow
          testID="open-preferences"
          icon="tune"
          title={t('mobile.accounts.preferences')}
          onPress={() => router.push(routes.settingsPreferences())}
        />
        <Divider inset={52} />
        <ListRow
          testID="open-info"
          icon="info"
          title={t('settings.info')}
          onPress={() => router.push(routes.settingsInfo())}
        />
      </ListSection>
    </>
  );
}
