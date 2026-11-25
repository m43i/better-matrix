// Making a pack in a room (modal, from the pack manager): its name. It opens once made.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';

import { t } from '../i18n';
import { routes } from '../routes';
import { createRoomPack } from '../state/actions/stickers';
import { useAppStore } from '../state/react';
import { TextField } from '../ui/components';
import { RoomForm } from '../ui/people/RoomForm';

export default function NewPackScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const valid = name.trim() !== '';

  const submit = () => {
    if (!valid || busy) return;
    setBusy(true);
    void createRoomPack(store, account, roomId, name).then((place) => {
      if (!place || place.type !== 'Room') {
        setBusy(false);

        return;
      }

      router.back();
      router.push(routes.pack(account, `${place.data.room_id}/${place.data.state_key}`, roomId));
    });
  };

  return (
    <RoomForm
      testID="new-pack"
      title={t('mobile.stickers.new_pack')}
      confirm={t('mobile.stickers.new_pack_create')}
      valid={valid}
      busy={busy}
      onSubmit={submit}
    >
      <TextField
        testID="new-pack-name"
        label={t('mobile.stickers.new_pack_name')}
        value={name}
        onChangeText={setName}
        autoFocus
        returnKeyType="done"
        onSubmitEditing={submit}
        maxLength={100}
      />
    </RoomForm>
  );
}
