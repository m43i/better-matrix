// Someone outside a room the member list shows apart: asking to join (Approve, Deny) or banned
// (Unban). The buttons show only with the right; the list decides that (`may`).

import type { AccountId, RoomMember } from '@better-matrix/matrix-native/types';
import { StyleSheet, Text, View } from 'react-native';

import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Button } from '../components';
import { usePalette } from '../theme';

export interface RequestAction {
  title: string;
  kind: 'primary' | 'secondary';
  onPress: () => void;
  testID: string;
}

export function RequestRow({
  account,
  member,
  actions,
}: {
  account: AccountId;
  member: RoomMember;
  actions: RequestAction[];
}) {
  const palette = usePalette();
  const { person } = member;

  return (
    <View style={styles.row} testID={`request-${person.id}`}>
      <Avatar account={account} id={person.id} name={person.name} media={person.avatar} size={40} />
      <View style={styles.text}>
        <Text style={[font.body, { color: palette.text }]} numberOfLines={1}>
          {person.name}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
          {person.id}
        </Text>
      </View>
      {actions.map((action) => (
        <Button
          key={action.testID}
          testID={action.testID}
          title={action.title}
          kind={action.kind}
          onPress={action.onPress}
          small
        />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: 16,
    paddingVertical: 8,
    minHeight: 56,
  },
  text: { flex: 1, minWidth: 0, gap: 1 },
});
