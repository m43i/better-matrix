// A permalink (sheet): the room it names, looked up for one account, with Join; or the person it
// names with Start chat. A room one of the accounts has joined opens instead (ui/links/open.ts
// opens one known by its ID before this screen; an alias is known once its preview names the
// room). With several accounts, the user picks which one acts. The desktop's link dialog says
// the same things (ADR 0008).

import type { AccountId, Person, RoomPreview } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { t, tp } from '../i18n';
import { findPerson, joinRoom, previewRoom } from '../state/actions/links';
import { openDirectChat } from '../state/actions/people';
import {
  joinedAfterPreview,
  joinedRoute,
  linkAccount,
  mayChooseAccount,
  type LinkStep,
  linkNote,
  offersJoin,
  shownPreview,
} from '../state/linkRoute';
import { parsePermalink } from '../state/permalinks';
import { shallow, useAppStore, useStore } from '../state/react';
import { roomPreviewPicture } from '../state/selectors';
import type { CommandFailed } from '../state/requests';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Button, Divider, EmptyState, ListRow, ListSection } from '../ui/components';
import { openJoined } from '../ui/links/open';
import { usePalette } from '../ui/theme';
import { useShown } from '../ui/useShown';

export default function LinkScreen() {
  const params = useLocalSearchParams<{ uri: string; account?: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const link = useMemo(() => parsePermalink(params.uri ?? ''), [params.uri]);
  const accounts = useStore((s) => s.accounts.map((a) => a.summary), shallow);

  const [account, setAccount] = useState<AccountId | null>(() =>
    linkAccount(store.getState(), params.account ?? null),
  );

  // Once the user picks the account, a room another account has joined no longer opens there.
  const [chosen, setChosen] = useState(false);

  const [step, setStep] = useState<LinkStep>(() =>
    link?.kind === 'user' ? { type: 'person', person: null, loading: true } : { type: 'loading' },
  );

  const requestKey = `${params.uri}|${account}|${chosen}`;
  const [previousRequest, setPreviousRequest] = useState(requestKey);

  if (previousRequest !== requestKey) {
    setPreviousRequest(requestKey);
    setStep(
      link?.kind === 'user' ? { type: 'person', person: null, loading: true } : { type: 'loading' },
    );
  }

  // A space opens as the account's scope on Home, a room at the linked message; not once the
  // user left the sheet while it waited (the join or chat still happened).
  const shown = useShown();

  const openRoom = useCallback(
    (owner: AccountId, roomId: string, space: boolean) => {
      if (!shown.current) return;
      router.dismiss();
      openJoined(store, joinedRoute(store.getState(), owner, roomId, space, link?.event ?? null));
    },
    [store, router, link, shown],
  );

  useEffect(() => {
    if (!link || !account) return;
    let current = true;

    if (link.kind === 'user') {
      const found = (person: Person | null) =>
        current && setStep({ type: 'person', person, loading: false });

      findPerson(store, account, link.id).then(found, () => found(null));
    } else {
      previewRoom(store, account, link.id, link.via).then(
        (preview) => {
          if (!current) return;
          const owner = joinedAfterPreview(store.getState(), preview, account, chosen);

          if (owner !== null) openRoom(owner, preview.room_id, preview.space);
          else setStep({ type: 'preview', preview });
        },
        (error: CommandFailed) => current && setStep({ type: 'failed', error: error.message }),
      );
    }

    return () => {
      current = false;
    };
  }, [link, account, chosen, store, openRoom]);

  if (!link) return <EmptyState icon="link" title={t('link.invalid')} />;

  if (!account) return null;

  const choose = (id: AccountId) => {
    if (id === account || !mayChooseAccount(step)) return;
    setChosen(true);
    setAccount(id);
  };

  const join = () => {
    if (step.type !== 'preview' && step.type !== 'failed') return;
    const before = step;
    const room = step.type === 'preview' ? step.preview.room_id : link.id;
    const space = step.type === 'preview' && step.preview.space;
    setStep({ type: 'joining', before });
    joinRoom(store, account, room, link.via).then(
      (roomId) => openRoom(account, roomId, space),
      () => setStep(before),
    );
  };

  const chat = () => {
    if (step.type !== 'person' || step.chatting) return;
    setStep({ ...step, chatting: true });
    openDirectChat(store, account, link.id).then(
      (roomId) => openRoom(account, roomId, false),
      () => setStep({ ...step, chatting: false }),
    );
  };

  return (
    <View testID="link-sheet" style={styles.content}>
      {step.type === 'loading' || (step.type === 'person' && step.loading) ? (
        <View style={styles.loading}>
          <ActivityIndicator color={palette.textMuted} />
          <Text style={[font.body, { color: palette.textMuted }]}>
            {t(step.type === 'person' ? 'link.person_loading' : 'link.loading')}
          </Text>
        </View>
      ) : step.type === 'person' ? (
        <PersonHead account={account} id={link.id} person={step.person} />
      ) : (
        <RoomHead account={account} id={link.id} preview={shownPreview(step)} />
      )}
      <Note step={step} />
      <View style={styles.actions}>
        {step.type === 'person' && !step.loading ? (
          <Button
            testID="link-chat"
            icon="chat"
            title={t('members.chat')}
            busy={step.chatting === true}
            onPress={chat}
          />
        ) : null}
        {offersJoin(step) ? (
          <Button
            testID="link-join"
            title={t(shownPreview(step)?.space ? 'link.join_space' : 'link.join')}
            busy={step.type === 'joining'}
            onPress={join}
          />
        ) : null}
      </View>
      {accounts.length > 1 ? (
        <ListSection title={t('link.as')} footer={t('mobile.links.as_footer')}>
          {accounts.map((summary, i) => (
            <View key={summary.id}>
              {i > 0 ? <Divider inset={52} /> : null}
              <ListRow
                testID={`link-account-${summary.id}`}
                leading={
                  <Avatar
                    account={summary.id}
                    id={summary.id}
                    name={summary.display_name}
                    media={summary.avatar}
                    size={28}
                  />
                }
                title={summary.display_name}
                subtitle={summary.id}
                selected={summary.id === account}
                onPress={() => choose(summary.id)}
                chevron={false}
              />
            </View>
          ))}
        </ListSection>
      ) : null}
    </View>
  );
}

/** The room's picture, name, address, member count and topic; its address alone without a preview. */
function RoomHead({
  account,
  id,
  preview,
}: {
  account: AccountId;
  id: string;
  preview: RoomPreview | null;
}) {
  const palette = usePalette();
  // An invited room's picture follows the account's choice for invitations (ADR 0011).
  const picture = useStore((s) => roomPreviewPicture(s, account, preview));
  const name = preview?.name ?? preview?.alias ?? id;
  const address = preview?.alias ?? (preview ? preview.room_id : null);

  return (
    <View style={styles.head}>
      <Avatar account={account} id={preview?.room_id ?? id} name={name} media={picture} size={72} />
      <Text style={[font.title, styles.centered, { color: palette.text }]} numberOfLines={2}>
        {name}
      </Text>
      {address && address !== name ? (
        <Text style={[font.body, styles.centered, { color: palette.textMuted }]} selectable>
          {address}
        </Text>
      ) : null}
      {preview ? (
        <Text style={[font.meta, { color: palette.textMuted }]}>
          {tp('overview.members', preview.member_count)}
        </Text>
      ) : null}
      {preview?.topic ? (
        <Text style={[font.body, styles.centered, { color: palette.text }]} numberOfLines={4}>
          {preview.topic}
        </Text>
      ) : null}
    </View>
  );
}

/** The person's picture, name and ID; the ID alone when their server didn't say. */
function PersonHead({
  account,
  id,
  person,
}: {
  account: AccountId;
  id: string;
  person: Person | null;
}) {
  const palette = usePalette();
  const name = person?.name ?? id;

  return (
    <View style={styles.head}>
      <Avatar account={account} id={id} name={name} media={person?.avatar} size={72} />
      <Text style={[font.title, styles.centered, { color: palette.text }]} numberOfLines={2}>
        {name}
      </Text>
      {name !== id ? (
        <Text style={[font.body, styles.centered, { color: palette.textMuted }]} selectable>
          {id}
        </Text>
      ) : null}
      {person === null ? (
        <Text style={[font.meta, styles.centered, { color: palette.textMuted }]}>
          {t('mobile.links.no_profile')}
        </Text>
      ) : null}
    </View>
  );
}

/** Why Join is or isn't offered, when it needs saying. */
function Note({ step }: { step: LinkStep }) {
  const palette = usePalette();
  const note = linkNote(step);

  if (note === null) return null;

  return <Text style={[font.body, styles.note, { color: palette.textMuted }]}>{t(note)}</Text>;
}

const styles = StyleSheet.create({
  content: { paddingTop: 28, paddingBottom: 32 },
  loading: { alignItems: 'center', gap: 12, paddingVertical: 24 },
  head: { alignItems: 'center', gap: 6, paddingHorizontal: 24 },
  centered: { textAlign: 'center' },
  note: { textAlign: 'center', marginTop: 16, marginHorizontal: 24 },
  actions: { marginTop: 20, marginHorizontal: 16, gap: 10 },
});
