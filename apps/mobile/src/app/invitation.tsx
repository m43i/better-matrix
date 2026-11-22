// An invitation's sheet, the desktop's invitation dialog: the room or space with its picture and
// topic, which account was invited and by whom, Decline and Accept; Ignore (decline and ignore
// the inviter) and Report for unwanted ones (ADR 0011). Answering waits for the engine;
// accepting opens the room (a space becomes the scope), and a failure stays here with its reason
// so it can be tried again. With pictures in invitations off, it shows initials.

import { router, useLocalSearchParams } from 'expo-router';
import { useState } from 'react';
import { Alert, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { afterAnswer, answerInvitation, type InvitationAnswer } from '../state/actions/invitations';
import { ignoreInviter } from '../state/actions/safety';
import { useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Button, Divider, ListRow, ListSection } from '../ui/components';
import { openJoined } from '../ui/links/open';
import { usePalette } from '../ui/theme';
import { useShown } from '../ui/useShown';

export default function InvitationScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const palette = usePalette();

  const invite = useStore((s) =>
    s.accounts.find((a) => a.summary.id === account)?.invitations.find((i) => i.room_id === roomId),
  );

  const pictures = useStore(
    (s) => s.accounts.find((a) => a.summary.id === account)?.safety.invite_pictures ?? true,
  );

  const [busy, setBusy] = useState<'accept' | 'decline' | 'ignore' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const shown = useShown();

  if (!invite || !account) return null;
  const room = !invite.space && invite.kind === 'Group';

  const answer = async (how: 'accept' | 'decline' | 'ignore') => {
    setBusy(how);
    setError(null);

    const result: InvitationAnswer =
      how === 'ignore'
        ? await ignoreInviter(store, account, invite)
        : await answerInvitation(store, account, invite.room_id, how === 'accept');

    setBusy(null);

    if (result.kind === 'failed') setError(result.error);
    const next = afterAnswer(result, invite.space, shown.current);

    if (next.type === 'stay') return;
    router.back();

    // As a link does: the invited account comes on screen, a space as its scope on Home.
    if (next.type === 'space') openJoined(store, { type: 'space', account, spaceId: next.spaceId });

    if (next.type === 'room')
      openJoined(store, { type: 'room', account, roomId: next.roomId, eventId: null });
  };

  const askIgnore = () => {
    const name = invite.inviter?.name ?? '';
    Alert.alert(
      t('mobile.safety.decline_ignore_title', { name }),
      t('mobile.safety.decline_ignore_body'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('safety.ignore'),
          style: 'destructive',
          onPress: () => void answer('ignore'),
        },
      ],
    );
  };

  const reportRoom = () => {
    router.back();
    router.push(
      routes.report(account, { type: 'Room', data: { room_id: invite.room_id } }, invite.name),
    );
  };

  return (
    <ScrollView style={{ backgroundColor: palette.bg }} contentContainerStyle={styles.content}>
      <View style={styles.head}>
        <Avatar
          account={account}
          id={invite.room_id}
          name={invite.name}
          media={pictures ? invite.avatar : null}
          size={64}
        />
        <Text
          style={[font.title, styles.center, { color: palette.text, fontSize: 24 }]}
          numberOfLines={2}
        >
          {room ? `# ${invite.name}` : invite.name}
        </Text>
        {invite.topic ? (
          <Text style={[font.body, styles.center, { color: palette.textMuted }]} numberOfLines={4}>
            {invite.topic}
          </Text>
        ) : null}
      </View>
      {invite.inviter ? (
        <Text style={[font.body, styles.center, { color: palette.text }]}>
          {t('invitations.from', { name: invite.inviter.name, user: invite.inviter.id })}
        </Text>
      ) : null}
      <Text style={[font.meta, styles.center, { color: palette.textMuted }]}>
        {t('invitations.account', { account })}
      </Text>
      {error ? (
        <Text style={[font.meta, styles.center, { color: palette.danger }]}>{error}</Text>
      ) : null}
      <View style={styles.buttons}>
        <View style={styles.button}>
          <Button
            testID="invitation-decline"
            title={t('invitations.decline')}
            kind="secondary"
            busy={busy === 'decline'}
            disabled={busy !== null}
            onPress={() => void answer('decline')}
          />
        </View>
        <View style={styles.button}>
          <Button
            testID="invitation-accept"
            title={t('invitations.accept')}
            busy={busy === 'accept'}
            disabled={busy !== null}
            onPress={() => void answer('accept')}
          />
        </View>
      </View>
      <ListSection>
        {invite.inviter ? (
          <>
            <ListRow
              testID="invitation-ignore"
              icon="block"
              title={t('mobile.safety.decline_ignore', { name: invite.inviter.name })}
              danger
              chevron={false}
              onPress={busy === null ? askIgnore : undefined}
            />
            <Divider inset={52} />
          </>
        ) : null}
        <ListRow
          testID="invitation-report"
          icon="flag"
          title={t('report.room_action')}
          danger
          chevron={false}
          onPress={busy === null ? reportRoom : undefined}
        />
      </ListSection>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: 24, gap: 12 },
  head: { alignItems: 'center', gap: 10 },
  center: { textAlign: 'center' },
  buttons: { flexDirection: 'row', gap: 12, marginTop: 8 },
  button: { flex: 1 },
});
