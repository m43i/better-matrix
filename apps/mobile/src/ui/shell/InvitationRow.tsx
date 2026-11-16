// One invitation in the room list's Invitations section, as the desktop's sidebar draws it:
// `#` for a room, the picture for a direct chat or space, the name in bold. Tapping opens the
// invitation sheet to accept or decline.

import type { AccountId, Invitation } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { memo } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Touchable } from '../components';
import { usePalette } from '../theme';

/** As large as the room rows' leading picture. */
const AVATAR = 22;

export const InvitationRow = memo(function InvitationRow({
  account,
  invitation,
}: {
  account: AccountId;
  invitation: Invitation;
}) {
  const palette = usePalette();
  const room = !invitation.space && invitation.kind === 'Group';

  return (
    <Touchable
      testID={`invitation-${invitation.room_id}`}
      accessibilityRole="button"
      accessibilityLabel={invitation.name}
      onPress={() => router.push(routes.invitation(account, invitation.room_id))}
      style={styles.row}
    >
      <View style={styles.leading}>
        {room ? (
          <Text style={[font.nav, styles.hash, { color: palette.textMuted }]}>#</Text>
        ) : (
          <Avatar
            account={account}
            id={invitation.room_id}
            name={invitation.name}
            media={invitation.avatar}
            size={AVATAR}
          />
        )}
      </View>
      <Text style={[font.navStrong, styles.name, { color: palette.text }]} numberOfLines={1}>
        {invitation.name}
      </Text>
    </Touchable>
  );
});

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: 16,
    paddingVertical: 9,
    minHeight: 42,
  },
  leading: { width: AVATAR, alignItems: 'center' },
  hash: { fontSize: 18, lineHeight: 22 },
  name: { flex: 1 },
});
