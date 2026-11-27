// The settings index groups device preferences and pages for the account on screen.

import { router } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { useStore } from '../../state/react';
import { Divider, ListRow, ListSection } from '../components';

export function SettingsRows() {
  const active = useStore((s) => s.active);

  return (
    <>
      <ListSection title={t('settings.preferences_group')}>
        <ListRow
          testID="open-general"
          icon="settings"
          title={t('settings.general')}
          onPress={() => router.push(routes.settingsGeneral())}
        />
        <Divider inset={52} />
        <ListRow
          testID="open-appearance"
          icon="tune"
          title={t('settings.appearance')}
          onPress={() => router.push(routes.settingsAppearance())}
        />
        <Divider inset={52} />
        <ListRow
          testID="open-messages-media"
          icon="chat"
          title={t('settings.messages_media')}
          onPress={() => router.push(routes.settingsMessagesMedia())}
        />
        <Divider inset={52} />
        <ListRow
          testID="open-notifications"
          icon="bell"
          title={t('settings.notifications')}
          onPress={() => router.push(routes.settingsNotifications())}
        />
      </ListSection>
      {active ? (
        <ListSection title={t('settings.account_group')}>
          <ListRow
            testID="open-profile"
            icon="user"
            title={t('settings.profile')}
            onPress={() => router.push(routes.settingsProfile(active))}
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
            testID="open-account"
            icon="shield"
            title={t('settings.account')}
            onPress={() => router.push(routes.settingsAccount(active))}
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
          testID="open-info"
          icon="info"
          title={t('settings.info')}
          onPress={() => router.push(routes.settingsInfo())}
        />
      </ListSection>
    </>
  );
}
