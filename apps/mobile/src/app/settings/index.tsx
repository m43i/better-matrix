// Settings: the way to every settings page, for the account on screen and this device.

import { t } from '../../i18n';
import { SettingsPage } from '../../ui/settings/common';
import { SettingsRows } from '../../ui/settings/SettingsRows';

export default function SettingsScreen() {
  return (
    <SettingsPage title={t('settings.title')} testID="settings">
      <SettingsRows />
    </SettingsPage>
  );
}
