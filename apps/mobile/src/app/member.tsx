// A member of a room (sheet, from the member list): who they are and their role, and what the
// user can do with them: message them, invite them to another room, and with the rights change
// their role, remove or ban them; and, for anyone, ignore or report them (ADR 0011). Destructive
// acts ask first, in the desktop's words.

import type { MemberRole, Moderation } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Alert, FlatList, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { invite, listAllMembers, moderate, openDirectChat } from '../state/actions/people';
import { setIgnored } from '../state/actions/safety';
import { asksFirst, inviteTargets, may, memberOf, rolesOffered, ROLES } from '../state/people';
import { shallow, useAppStore, useStore } from '../state/react';
import { ignores } from '../state/safety';
import { account as accountOf } from '../state/selectors';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Divider, ListRow, ListSection } from '../ui/components';
import { Icon } from '../ui/icons';
import { roleLabel } from '../ui/people/labels';
import { usePalette } from '../ui/theme';

/** Which of the sheet's lists is open: none, the rooms to invite to, or the roles. */
type Open = 'none' | 'invite' | 'role';

export default function MemberScreen() {
  const { account, roomId, userId } = useLocalSearchParams<{
    account: string;
    roomId: string;
    userId: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const members = useStore((s) => accountOf(s, account)?.members[roomId]);

  const ignored = useStore((s) => {
    const safety = accountOf(s, account)?.safety;

    return safety !== undefined && ignores(safety, userId);
  });

  const rooms = useStore((s) => inviteTargets(accountOf(s, account)?.rooms ?? [], roomId), shallow);
  const [open, setOpen] = useState<Open>('none');
  const [busy, setBusy] = useState<'chat' | MemberRole | null>(null);
  // A mention opens this sheet straight from the timeline, before anything listed the members.
  useEffect(() => listAllMembers(store, account, roomId), [store, account, roomId]);
  const member = memberOf(members, userId);

  if (!members || !member) return null;
  const { person } = member;
  const me = person.id === account;
  const roles = rolesOffered(members, member);
  const mayKick = may(members, { type: 'Kick' }, member);
  const mayBan = may(members, { type: 'Ban' }, member);

  const message = () => {
    if (busy !== null) return;
    setBusy('chat');
    openDirectChat(store, account, person.id).then(
      (chat) => {
        router.dismiss();
        router.push(routes.room(account, chat));
      },
      () => setBusy(null),
    );
  };

  const inviteTo = (target: string, name: string) => {
    setOpen('none');
    invite(store, account, target, person.id).then(
      () => Alert.alert(t('mobile.people.invited_to', { name: person.name, room: name })),
      () => {},
    );
  };

  const act = (action: Moderation) => {
    const run = () => {
      if (action.type === 'Role') {
        setBusy(action.data);
        moderate(store, account, roomId, person.id, action).then(
          () => setBusy(null),
          () => setBusy(null),
        );
      } else {
        router.back();
        moderate(store, account, roomId, person.id, action).catch(() => {});
      }
    };

    if (!asksFirst(action, members.own_level)) {
      run();

      return;
    }

    const [title, body, confirm] =
      action.type === 'Kick'
        ? [
            t('confirm.kick_title', { name: person.name }),
            t('confirm.kick_body'),
            t('confirm.kick'),
          ]
        : action.type === 'Ban'
          ? [t('confirm.ban_title', { name: person.name }), t('confirm.ban_body'), t('confirm.ban')]
          : [
              t('confirm.promote_title', { name: person.name }),
              t('confirm.promote_body'),
              t('confirm.promote'),
            ];

    Alert.alert(title, body, [
      { text: t('common.cancel'), style: 'cancel' },
      { text: confirm, style: 'destructive', onPress: run },
    ]);
  };

  const ignore = () => {
    if (ignored) {
      void setIgnored(store, account, person.id, false).catch(() => {});

      return;
    }

    Alert.alert(
      t('mobile.safety.ignore_title', { name: person.name }),
      t('mobile.safety.ignore_body'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('mobile.safety.ignore'),
          style: 'destructive',
          onPress: () => void setIgnored(store, account, person.id, true).catch(() => {}),
        },
      ],
    );
  };

  const reportPerson = () => {
    router.back();
    router.push(
      routes.report(account, { type: 'Person', data: { user_id: person.id } }, person.name),
    );
  };

  const toggle = (which: Open) => setOpen((current) => (current === which ? 'none' : which));

  return (
    // A plain view, not a scroll view: the sheet fits its content.
    <View testID="member-sheet" style={styles.content}>
      <View style={styles.head}>
        <Avatar
          account={account}
          id={person.id}
          name={person.name}
          media={person.avatar}
          size={72}
          presence={member.presence}
        />
        <View style={styles.name}>
          <Text style={[font.title, { color: palette.text }]} numberOfLines={2}>
            {person.name}
          </Text>
          {member.verified ? (
            <View accessible accessibilityLabel={t('members.verified')}>
              <Icon name="verified" size={20} color={palette.success} />
            </View>
          ) : null}
        </View>
        <Text style={[font.body, { color: palette.textMuted }]} selectable>
          {person.id}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]}>
          {me ? t('mobile.people.you') : roleLabel(member.role)}
        </Text>
      </View>
      {me ? null : (
        <>
          <ListSection>
            <ListRow
              testID="member-chat"
              icon="chat"
              title={t('members.chat')}
              onPress={message}
              chevron={false}
              leading={
                busy === 'chat' ? <ActivityIndicator color={palette.textMuted} /> : undefined
              }
            />
            <Divider inset={52} />
            <ListRow
              testID="member-invite"
              icon="userAdd"
              title={t('members.invite_to')}
              onPress={() => toggle('invite')}
              chevron={false}
            />
            {open === 'invite' ? (
              <FlatList
                style={styles.nested}
                nestedScrollEnabled
                data={rooms}
                keyExtractor={(r) => r.room_id}
                ListEmptyComponent={
                  <Text style={[font.body, styles.note, { color: palette.textMuted }]}>
                    {t('mobile.people.no_rooms')}
                  </Text>
                }
                renderItem={({ item: r }) => (
                  <ListRow
                    testID={`member-invite-${r.room_id}`}
                    leading={
                      <Avatar
                        account={account}
                        id={r.room_id}
                        name={r.name}
                        media={r.avatar}
                        size={28}
                      />
                    }
                    title={r.name}
                    onPress={() => inviteTo(r.room_id, r.name)}
                    chevron={false}
                  />
                )}
              />
            ) : null}
          </ListSection>
          {roles.length > 0 || mayKick || mayBan ? (
            <ListSection>
              {roles.length > 0 ? (
                <ListRow
                  testID="member-role"
                  icon="shield"
                  title={t('members.role')}
                  value={roleLabel(member.role)}
                  onPress={() => toggle('role')}
                  chevron={false}
                />
              ) : null}
              {open === 'role'
                ? ROLES.filter((role) => role === member.role || roles.includes(role)).map(
                    (role) => (
                      <ListRow
                        key={role}
                        testID={`member-role-${role}`}
                        title={roleLabel(role)}
                        selected={role === member.role}
                        leading={
                          busy === role ? (
                            <ActivityIndicator color={palette.textMuted} />
                          ) : (
                            <View style={styles.indent} />
                          )
                        }
                        onPress={
                          role === member.role ? undefined : () => act({ type: 'Role', data: role })
                        }
                        chevron={false}
                      />
                    ),
                  )
                : null}
              {mayKick ? (
                <>
                  {roles.length > 0 ? <Divider inset={52} /> : null}
                  <ListRow
                    testID="member-kick"
                    icon="leave"
                    title={t('members.kick')}
                    danger
                    onPress={() => act({ type: 'Kick' })}
                    chevron={false}
                  />
                </>
              ) : null}
              {mayBan ? (
                <>
                  {roles.length > 0 || mayKick ? <Divider inset={52} /> : null}
                  <ListRow
                    testID="member-ban"
                    icon="alert"
                    title={t('members.ban')}
                    danger
                    onPress={() => act({ type: 'Ban' })}
                    chevron={false}
                  />
                </>
              ) : null}
            </ListSection>
          ) : null}
          <ListSection>
            <ListRow
              testID="member-ignore"
              icon="block"
              title={ignored ? t('mobile.safety.unignore') : t('mobile.safety.ignore')}
              danger={!ignored}
              onPress={ignore}
              chevron={false}
            />
            <Divider inset={52} />
            <ListRow
              testID="member-report"
              icon="flag"
              title={t('mobile.safety.report')}
              danger
              onPress={reportPerson}
              chevron={false}
            />
          </ListSection>
        </>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingTop: 28, paddingBottom: 32 },
  head: { alignItems: 'center', gap: 6, paddingHorizontal: 24 },
  name: { flexDirection: 'row', alignItems: 'center', gap: 6, marginTop: 8 },
  nested: { maxHeight: 280 },
  note: { padding: 16 },
  indent: { width: 22 },
});
