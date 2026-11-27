import { router } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { updateSettings } from '../../state/actions/core';
import { useAppStore, useStore } from '../../state/react';
import { ListRow, ListSection } from '../../ui/components';
import { SettingsPage } from '../../ui/settings/common';

export default function MessagesMediaScreen() {
  const store = useAppStore();
  const settings = useStore((s) => s.settings);
  const giphyKey = useStore((s) => s.giphyKey !== null);

  return (
    <SettingsPage title={t('settings.messages_media')} testID="settings-messages-media">
      <ListSection footer={`${t('settings.previews_hint')} ${t('settings.previews_on_hint')}`}>
        <ListRow
          testID="previews"
          icon="link"
          title={t('settings.previews')}
          toggle={{
            value: settings.showLinkPreviews,
            onChange: (on) => updateSettings(store, { showLinkPreviews: on }),
          }}
        />
      </ListSection>
      <ListSection footer={`${t('settings.gifs_hint')} ${t('settings.gifs_on_hint')}`}>
        <ListRow
          testID="gifs"
          icon="image"
          title={t('settings.gifs')}
          toggle={{
            value: settings.showLinkGifs,
            onChange: (on) => updateSettings(store, { showLinkGifs: on }),
          }}
        />
      </ListSection>
      <ListSection footer={t('mobile.stickers.giphy_hint')}>
        <ListRow
          testID="giphy-key"
          icon="key"
          title={t('mobile.stickers.giphy_title')}
          value={t(giphyKey ? 'mobile.stickers.giphy_set' : 'mobile.stickers.giphy_not_set')}
          onPress={() => router.push(routes.settingsGiphy())}
        />
      </ListSection>
      <ListSection footer={t('mobile.accounts.maps_on_hint')}>
        <ListRow
          testID="maps"
          icon="location"
          title={t('mobile.accounts.maps')}
          toggle={{
            value: settings.showMaps,
            onChange: (on) => updateSettings(store, { showMaps: on }),
          }}
        />
      </ListSection>
      <ListSection footer={t('settings.room_changes_hint')}>
        <ListRow
          testID="room-changes"
          icon="history"
          title={t('settings.room_changes')}
          toggle={{
            value: settings.showRoomChanges,
            onChange: (on) => updateSettings(store, { showRoomChanges: on }),
          }}
        />
      </ListSection>
    </SettingsPage>
  );
}
