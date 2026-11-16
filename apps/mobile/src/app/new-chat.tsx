// A new direct chat (modal): find someone on the active account's server and open the chat with
// them, started when there is none. The chat replaces this modal.

import { Stack, useRouter } from 'expo-router';
import { useState } from 'react';
import { ActivityIndicator } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { openDirectChat } from '../state/actions/people';
import { useAppStore, useStore } from '../state/react';
import { PeoplePicker } from '../ui/people/PeoplePicker';
import { usePalette } from '../ui/theme';

export default function NewChatScreen() {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const account = useStore((s) => s.active);
  const [opening, setOpening] = useState<string | null>(null);
  if (account === null) return null;
  const open = (userId: string) => {
    if (opening !== null) return;
    setOpening(userId);
    openDirectChat(store, account, userId).then(
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
        onPick={(person) => open(person.id)}
        trailing={(person) => (opening === person.id ? <ActivityIndicator color={palette.textMuted} /> : null)}
      />
    </>
  );
}
