// The one question before a room is left, or a direct chat deleted: the room menu's, room
// info's and `/leave`'s (the desktop's `Confirm::LeaveRoom` and `Confirm::DeleteChat`).

import type { RoomSummary } from '@better-matrix/matrix-native/types';
import { Alert } from 'react-native';

import { t } from '../i18n';

/**
 * Asks whether to leave `room`, or for a direct chat whether to delete it. `leave` runs once the
 * user agreed, told whether to forget the room (a deleted chat is); cancelling does nothing.
 */
export function askToLeave(
  room: Pick<RoomSummary, 'name' | 'kind'>,
  leave: (forget: boolean) => void,
): void {
  const direct = room.kind === 'Direct';

  const [title, body, action] = direct
    ? [
        t('confirm.delete_title', { name: room.name }),
        t('confirm.delete_body', { name: room.name }),
        t('confirm.delete'),
      ]
    : [t('confirm.leave_title', { room: room.name }), t('confirm.leave_body'), t('confirm.leave')];

  Alert.alert(title, body, [
    { text: t('common.cancel'), style: 'cancel' },
    { text: action, style: 'destructive', onPress: () => leave(direct) },
  ]);
}
