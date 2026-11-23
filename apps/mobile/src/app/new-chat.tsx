// A new direct chat (modal): find someone on the active account's server and open the chat with
// them, started when there is none; the user's note to self comes first. The chat replaces this
// modal.

import { Stack, useRouter } from 'expo-router';
import { useState } from 'react';
import { ActivityIndicator } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { openDirectChat, openNoteToSelf } from '../state/actions/people';
import { offersNoteToSelf } from '../state/people';
import { useAppStore, useStore } from '../state/react';
import { account as accountData } from '../state/selectors';
import { PeoplePicker } from '../ui/people/PeoplePicker';
import { PersonRow } from '../ui/people/rows';
import { usePalette } from '../ui/theme';

export default function NewChatScreen() {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const account = useStore((s) => s.active);
  const me = useStore((s) => accountData(s, account)?.summary);
  const [opening, setOpening] = useState<string | null>(null);

  if (account === null) return null;

  const busy = (userId: string) =>
    opening === userId ? <ActivityIndicator color={palette.textMuted} /> : null;

  // `key` marks the row opening: a person's ID, or the user's own for the note to self.
  const open = (key: string, chat: () => Promise<string>) => {
    if (opening !== null) return;
    setOpening(key);
    chat().then(
      (roomId) => router.replace(routes.room(account, roomId)),
      // The notice says why; the user may pick someone else.
      () => setOpening(null),
    );
  };

  return (
    <>
      <Stack.Screen options={{ title: t('create.chat') }} />
      <PeoplePicker
        testID="new-chat"
        account={account}
        placeholder={t('create.chat_placeholder')}
        onPick={(person) => open(person.id, () => openDirectChat(store, account, person.id))}
        trailing={(person) => busy(person.id)}
        first={(typed) =>
          offersNoteToSelf(typed, account, t('note_to_self.name')) ? (
            <PersonRow
              account={account}
              person={{ id: account, name: t('note_to_self.name'), avatar: me?.avatar ?? null }}
              subtitle={t('note_to_self.hint')}
              trailing={busy(account)}
              onPress={() => open(account, () => openNoteToSelf(store, account))}
              testID="new-chat-note-to-self"
            />
          ) : null
        }
      />
    </>
  );
}
