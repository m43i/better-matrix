// Renaming a room or space (modal): its current name, ready to change.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';

import { t } from '../i18n';
import { renameRoom } from '../state/actions/people';
import { renameCommand } from '../state/people';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf } from '../state/selectors';
import { TextField } from '../ui/components';
import { RoomForm } from '../ui/people/RoomForm';

export default function RenameRoomScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const current = useStore((s) => {
    const data = accountOf(s, account);
    return data?.rooms.find((r) => r.room_id === roomId)?.name ?? data?.spaces.find((sp) => sp.room_id === roomId)?.name ?? '';
  });
  const [name, setName] = useState(current);
  const [busy, setBusy] = useState(false);
  const command = renameCommand(account, roomId, current, name);
  const submit = () => {
    if (command === null || busy) return;
    setBusy(true);
    renameRoom(store, command).then(
      () => router.back(),
      () => setBusy(false),
    );
  };
  return (
    <RoomForm testID="rename-room" title={t('create.rename_title')} confirm={t('common.save')} valid={command !== null} busy={busy} onSubmit={submit}>
      <TextField
        testID="rename-room-name"
        label={t('create.name')}
        value={name}
        onChangeText={setName}
        autoFocus
        selectTextOnFocus
        returnKeyType="done"
        onSubmitEditing={submit}
        maxLength={255}
      />
    </RoomForm>
  );
}
