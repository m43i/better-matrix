// Pinning a web page to a room as a widget (modal): its name, optional, and its address.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';

import { t } from '../i18n';
import { addWidget } from '../state/actions/developer';
import { useAppStore } from '../state/react';
import { TextField } from '../ui/components';
import { RoomForm } from '../ui/people/RoomForm';

export default function AddWidgetScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState(false);
  const valid = url.trim() !== '';

  const submit = () => {
    if (!valid || busy) return;
    setBusy(true);
    addWidget(store, account, roomId, name, url).then(
      () => router.back(),
      () => setBusy(false),
    );
  };

  return (
    <RoomForm
      testID="add-widget"
      title={t('widgets.add_title')}
      confirm={t('widgets.add_confirm')}
      valid={valid}
      busy={busy}
      onSubmit={submit}
    >
      <TextField
        testID="add-widget-name"
        label={t('widgets.name')}
        value={name}
        onChangeText={setName}
        autoFocus
        returnKeyType="next"
        maxLength={255}
      />
      <TextField
        testID="add-widget-url"
        label={t('widgets.url')}
        value={url}
        onChangeText={setUrl}
        autoCapitalize="none"
        autoCorrect={false}
        keyboardType="url"
        returnKeyType="done"
        onSubmitEditing={submit}
      />
    </RoomForm>
  );
}
