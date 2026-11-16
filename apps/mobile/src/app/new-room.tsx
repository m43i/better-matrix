// A new room (modal): its name, an optional topic, and the space it goes in (none, or one the
// user may add rooms to; the `space` param picks one ahead). The new room replaces this modal.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';

import { t } from '../i18n';
import { routes } from '../routes';
import { create } from '../state/actions/people';
import { createRoomCommand, spacesForNewRoom } from '../state/people';
import { shallow, useAppStore, useStore } from '../state/react';
import { activeAccount } from '../state/selectors';
import { Avatar } from '../ui/Avatar';
import { ListRow, ListSection, TextField } from '../ui/components';
import { RoomForm } from '../ui/people/RoomForm';

export default function NewRoomScreen() {
  const params = useLocalSearchParams<{ space?: string }>();
  const store = useAppStore();
  const router = useRouter();
  const account = useStore((s) => s.active);
  const spaces = useStore((s) => spacesForNewRoom(activeAccount(s)?.spaces ?? []), shallow);
  const [name, setName] = useState('');
  const [topic, setTopic] = useState('');
  const [space, setSpace] = useState<string | null>(params.space ?? null);
  const [busy, setBusy] = useState(false);
  if (account === null) return null;
  const command = createRoomCommand(account, { name, topic, space });
  const submit = () => {
    if (command === null || busy) return;
    setBusy(true);
    create(store, command).then(
      (roomId) => router.replace(routes.room(account, roomId)),
      () => setBusy(false),
    );
  };
  const chosen = spaces.find((s) => s.room_id === space);
  return (
    <RoomForm testID="new-room" title={t('create.room_title')} confirm={t('create.create')} valid={command !== null} busy={busy} onSubmit={submit}>
      <TextField
        testID="new-room-name"
        label={t('create.name')}
        value={name}
        onChangeText={setName}
        autoFocus
        returnKeyType="next"
        maxLength={255}
      />
      <TextField
        testID="new-room-topic"
        label={t('create.topic')}
        value={topic}
        onChangeText={setTopic}
        returnKeyType="done"
        onSubmitEditing={submit}
      />
      {spaces.length > 0 ? (
        <ListSection title={t('mobile.people.space')} footer={chosen ? t('create.room_in', { space: chosen.name }) : undefined}>
          <ListRow
            testID="new-room-space-none"
            title={t('mobile.people.no_space')}
            icon="home"
            selected={space === null}
            chevron={false}
            onPress={() => setSpace(null)}
          />
          {spaces.map((s) => (
            <ListRow
              key={s.room_id}
              testID={`new-room-space-${s.room_id}`}
              title={s.name}
              leading={<Avatar account={account} id={s.room_id} name={s.name} media={s.avatar} size={28} />}
              selected={space === s.room_id}
              chevron={false}
              onPress={() => setSpace(s.room_id)}
            />
          ))}
        </ListSection>
      ) : null}
    </RoomForm>
  );
}
