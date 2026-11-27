import { Fragment } from 'react';

import { t } from '../../i18n';
import { updateSettings } from '../../state/actions/core';
import { useAppStore, useStore } from '../../state/react';
import { Divider, ListRow, ListSection } from '../../ui/components';
import { SettingsPage } from '../../ui/settings/common';
import { APPEARANCES, MESSAGE_LAYOUTS } from '../../ui/settings/choices';

export default function AppearanceScreen() {
  const store = useAppStore();
  const settings = useStore((s) => s.settings);

  return (
    <SettingsPage title={t('settings.appearance')} testID="settings-appearance">
      <ListSection title={t('settings.theme')} footer={t('settings.theme_hint')}>
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
      <ListSection title={t('settings.messages')} footer={t('settings.messages_hint')}>
        {MESSAGE_LAYOUTS.map((choice, index) => (
          <Fragment key={choice.id}>
            {index > 0 ? <Divider inset={16} /> : null}
            <ListRow
              testID={`message-layout-${choice.id}`}
              title={t(choice.label)}
              subtitle={t(choice.hint)}
              selected={settings.messageLayout === choice.id}
              onPress={() => updateSettings(store, { messageLayout: choice.id })}
              chevron={false}
            />
          </Fragment>
        ))}
      </ListSection>
    </SettingsPage>
  );
}
