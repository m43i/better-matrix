// Rows of people, as the member list and the people searches show them.

import type { AccountId, Person, RoomMember } from '@better-matrix/matrix-native/types';
import type { ReactNode } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Touchable } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { roleLabel } from './labels';

/** Someone found or suggested: their picture, name and user ID, and what follows them. */
export function PersonRow({
  account,
  person,
  trailing,
  onPress,
  testID,
}: {
  account: AccountId;
  person: Person;
  trailing?: ReactNode;
  onPress?: () => void;
  testID?: string;
}) {
  const palette = usePalette();
  const body = (
    <View style={styles.row}>
      <Avatar account={account} id={person.id} name={person.name} media={person.avatar} size={40} />
      <View style={styles.text}>
        <Text style={[font.body, { color: palette.text }]} numberOfLines={1}>
          {person.name}
        </Text>
        {person.name !== person.id ? (
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {person.id}
          </Text>
        ) : null}
      </View>
      {trailing}
    </View>
  );
  if (!onPress) return <View testID={testID}>{body}</View>;
  return (
    <Touchable testID={testID} onPress={onPress} accessibilityRole="button" accessibilityLabel={person.name}>
      {body}
    </Touchable>
  );
}

/**
 * One member of a room: their picture with a presence dot, name, a badge when verified, and
 * their role (members have none shown, as on the desktop).
 */
export function MemberRow({
  account,
  member,
  me,
  onPress,
}: {
  account: AccountId;
  member: RoomMember;
  me: boolean;
  onPress: () => void;
}) {
  const palette = usePalette();
  const { person } = member;
  const body = (
    <View style={styles.row}>
      <Avatar account={account} id={person.id} name={person.name} media={person.avatar} size={40} presence={member.presence} />
      <View style={styles.text}>
        <Text style={[font.body, { color: palette.text }]} numberOfLines={1}>
          {person.name}
        </Text>
        {me ? <Text style={[font.meta, { color: palette.textMuted }]}>{t('mobile.people.you')}</Text> : null}
      </View>
      {member.verified ? (
        <View accessible accessibilityLabel={t('members.verified')}>
          <Icon name="verified" size={18} color={palette.success} />
        </View>
      ) : null}
      {member.role !== 'Member' ? <Text style={[font.meta, { color: palette.textMuted }]}>{roleLabel(member.role)}</Text> : null}
    </View>
  );
  // The user's own row leads nowhere, as on the desktop.
  if (me) return <View testID={`member-${person.id}`}>{body}</View>;
  return (
    <Touchable testID={`member-${person.id}`} onPress={onPress} accessibilityRole="button" accessibilityLabel={person.name}>
      {body}
    </Touchable>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 16, paddingVertical: 8, minHeight: 56 },
  text: { flex: 1, minWidth: 0, gap: 1 },
});
