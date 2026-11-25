// The user's own GIPHY API key (from Preferences): entering, changing or removing it.

import { router } from 'expo-router';

import { t } from '../../i18n';
import { SettingsPage } from '../../ui/settings/common';
import { GiphyKeyForm } from '../../ui/stickers/GiphyKeyForm';

export default function GiphySettingsScreen() {
  return (
    <SettingsPage title={t('mobile.stickers.giphy_title')} testID="settings-giphy">
      <GiphyKeyForm onSaved={() => router.back()} />
    </SettingsPage>
  );
}
