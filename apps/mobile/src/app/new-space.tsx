// A new space (modal): its name. Once made, the account's room list shows it, on Home.

import { useRouter } from 'expo-router';
import { useState } from 'react';

import { t } from '../i18n';
import { routes } from '../routes';
import { createSpace } from '../state/actions/people';
import { createSpaceCommand } from '../state/people';
import { useAppStore, useStore } from '../state/react';
import { TextField } from '../ui/components';
import { RoomForm } from '../ui/people/RoomForm';

export default function NewSpaceScreen() {
  const store = useAppStore();
  const router = useRouter();
  const account = useStore((s) => s.active);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);

  if (account === null) return null;
  const command = createSpaceCommand(account, name);

  const submit = () => {
    if (command === null || busy) return;
    setBusy(true);
    createSpace(store, command).then(
      () => router.dismissTo(routes.home()),
      () => setBusy(false),
    );
  };

  return (
    <RoomForm
      testID="new-space"
      title={t('create.space_title')}
      confirm={t('create.create')}
      valid={command !== null}
      busy={busy}
      onSubmit={submit}
    >
      <TextField
        testID="new-space-name"
        label={t('create.name')}
        value={name}
        onChangeText={setName}
        autoFocus
        returnKeyType="done"
        onSubmitEditing={submit}
        maxLength={255}
      />
    </RoomForm>
  );
}
