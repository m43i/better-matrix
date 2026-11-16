// Inviting people to a room (modal): find them in the directory or type their user ID, and
// invite with a tap. Each invited person shows as such, and the user stays to invite more.

import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Text } from 'react-native';

import { t } from '../i18n';
import { invite, listAllMembers } from '../state/actions/people';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf, room as roomOf } from '../state/selectors';
import { font } from '../theme/palette';
import { PeoplePicker } from '../ui/people/PeoplePicker';
import { HeaderButton } from '../ui/people/RoomForm';
import { usePalette } from '../ui/theme';

/** How inviting someone went so far: on its way, or done. */
type Status = 'inviting' | 'invited';

export default function InviteScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const room = useStore((s) => roomOf(s, account, roomId));
  const members = useStore((s) => accountOf(s, account)?.members[roomId]);
  const [status, setStatus] = useState<Record<string, Status>>({});
  // Who is in the room already shows as such in the results.
  useEffect(() => listAllMembers(store, account, roomId), [store, account, roomId]);

  const pick = (userId: string) => {
    if (status[userId] !== undefined || members?.people.some((m) => m.person.id === userId)) return;
    setStatus((s) => ({ ...s, [userId]: 'inviting' }));
    invite(store, account, roomId, userId).then(
      () => setStatus((s) => ({ ...s, [userId]: 'invited' })),
      () =>
        setStatus((s) => {
          const { [userId]: _failed, ...rest } = s;

          return rest;
        }),
    );
  };

  return (
    <>
      <Stack.Screen
        options={{
          title: t('invite.title', { room: room?.name ?? roomId }),
          headerRight: () => (
            <HeaderButton
              title={t('common.done')}
              onPress={() => router.back()}
              strong
              testID="invite-done"
            />
          ),
        }}
      />
      <PeoplePicker
        testID="invite"
        account={account}
        placeholder={t('invite.placeholder')}
        onPick={(person) => pick(person.id)}
        trailing={(person) => {
          if (members?.people.some((m) => m.person.id === person.id)) {
            return (
              <Text style={[font.meta, { color: palette.textMuted }]}>{t('invite.member')}</Text>
            );
          }

          switch (status[person.id]) {
            case 'inviting':
              return <ActivityIndicator color={palette.textMuted} />;
            case 'invited':
              return (
                <Text style={[font.meta, { color: palette.success }]}>{t('invite.invited')}</Text>
              );
            default:
              return (
                <Text style={[font.bodyStrong, { color: palette.accent }]}>
                  {t('invite.invite')}
                </Text>
              );
          }
        }}
      />
    </>
  );
}
