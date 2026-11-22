// Preferences on this device: language, appearance, link previews and GIFs from links, and hidden
// events (developer tools), as the desktop's General settings.

import { Fragment } from 'react';

import { t } from '../../i18n';
import { updateSettings } from '../../state/actions/core';
import { useAppStore, useStore } from '../../state/react';
import { Divider, ListRow, ListSection } from '../../ui/components';
import { APPEARANCES, languageChoices } from '../../ui/settings/choices';
import { SettingsPage } from '../../ui/settings/common';

export default function PreferencesScreen() {
  const store = useAppStore();
  const settings = useStore((s) => s.settings);

  return (
    <SettingsPage title={t('mobile.accounts.preferences')} testID="preferences">
      <ListSection title={t('settings.language')} footer={t('settings.language_hint')}>
        {languageChoices().map((choice, index) => (
          <Fragment key={choice.id}>
            {index > 0 ? <Divider inset={16} /> : null}
            <ListRow
              testID={`language-${choice.id}`}
              title={choice.label}
              selected={settings.language === choice.id}
              onPress={() => updateSettings(store, { language: choice.id })}
              chevron={false}
            />
          </Fragment>
        ))}
      </ListSection>
      <ListSection title={t('settings.appearance')} footer={t('settings.appearance_hint')}>
        {APPEARANCES.map((choice, index) => (
          <Fragment key={choice.id}>
            {index > 0 ? <Divider inset={16} /> : null}
            <ListRow
              testID={`appearance-${choice.id}`}
              title={t(choice.label)}
              selected={settings.appearance === choice.id}
              onPress={() => updateSettings(store, { appearance: choice.id })}
              chevron={false}
            />
          </Fragment>
        ))}
      </ListSection>
      <ListSection
        title={t('settings.previews')}
        footer={
          settings.showLinkPreviews ? t('settings.previews_on_hint') : t('settings.previews_hint')
        }
      >
        <ListRow
          testID="previews"
          icon="link"
          title={t('settings.previews_on')}
          toggle={{
            value: settings.showLinkPreviews,
            onChange: (on) => updateSettings(store, { showLinkPreviews: on }),
          }}
        />
      </ListSection>
      <ListSection
        title={t('settings.gifs')}
        footer={settings.showLinkGifs ? t('settings.gifs_on_hint') : t('settings.gifs_hint')}
      >
        <ListRow
          testID="gifs"
          icon="image"
          title={t('settings.gifs_on')}
          toggle={{
            value: settings.showLinkGifs,
            onChange: (on) => updateSettings(store, { showLinkGifs: on }),
          }}
        />
      </ListSection>
      <ListSection
        title={t('settings.developer')}
        footer={
          settings.showHiddenEvents
            ? t('settings.hidden_events_on_hint')
            : t('settings.developer_hint')
        }
      >
        <ListRow
          testID="hidden-events"
          icon="code"
          title={t('settings.hidden_events_on')}
          toggle={{
            value: settings.showHiddenEvents,
            onChange: (on) => updateSettings(store, { showHiddenEvents: on }),
          }}
        />
      </ListSection>
    </SettingsPage>
  );
}
