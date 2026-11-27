import { Fragment } from 'react';

import { t } from '../../i18n';
import { updateSettings } from '../../state/actions/core';
import { useAppStore, useStore } from '../../state/react';
import { Divider, ListRow, ListSection } from '../../ui/components';
import { SettingsPage } from '../../ui/settings/common';
import { languageChoices } from '../../ui/settings/choices';

export default function GeneralScreen() {
  const store = useAppStore();
  const settings = useStore((s) => s.settings);

  return (
    <SettingsPage title={t('settings.general')} testID="settings-general">
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
      <ListSection title={t('settings.developer')} footer={t('settings.hidden_events_on_hint')}>
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
