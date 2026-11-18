// Who has read up to a message, as the desktop shows it in a narrow window: up to two faces and
// how many more, in a column every room message keeps free at its right (`READERS_COLUMN`), so
// a receipt never rewraps or grows its message. A tap lists them all (`app/readers.tsx`).

import type { Message, Person } from '@better-matrix/matrix-native/types';
import { useRouter } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { tp } from '../../i18n';
import { routes } from '../../routes';
import { READER_FACES, readerOf } from '../../state/readers';
import { shallow, useStore } from '../../state/react';
import { account as accountOf } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Touchable } from '../components';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';

const FACE = 16;

const FACE_OVERLAP = 6;

/** Room for the overflow count ("+99") after the faces. */
const COUNT = 22;

/** The width every room message keeps free at its right for its read indicator. */
export const READERS_COLUMN = FACE + (READER_FACES - 1) * (FACE - FACE_OVERLAP) + 6 + COUNT + 8;

export function ReadIndicator({ message }: { message: Message }) {
  const palette = usePalette();
  const router = useRouter();
  const { account, timelineKey } = useTimelineContext();
  const receipts = message.read_receipts ?? [];

  const faces = useStore(
    (s): Person[] =>
      receipts
        .slice(0, READER_FACES)
        .map((r) => readerOf(accountOf(s, account), timelineKey.room_id, r.user_id)),
    shallow,
  );

  const label = tp('message.seen_by', receipts.length);
  const more = receipts.length - READER_FACES;

  return (
    <Touchable
      testID={`readers-${message.event_id}`}
      onPress={() => router.push(routes.readers(account, timelineKey.room_id, message.event_id))}
      accessibilityRole="button"
      accessibilityLabel={label}
      style={styles.button}
    >
      <View style={styles.faces}>
        {faces.map((person, index) => (
          <View
            key={person.id}
            style={[
              styles.face,
              { borderColor: palette.bg, marginLeft: index === 0 ? 0 : -FACE_OVERLAP },
            ]}
          >
            <Avatar
              account={account}
              id={person.id}
              name={person.name}
              media={person.avatar}
              size={FACE}
            />
          </View>
        ))}
      </View>
      {more > 0 ? (
        <Text style={[font.meta, { color: palette.textMuted }]}>{`+${more}`}</Text>
      ) : null}
    </Touchable>
  );
}

const styles = StyleSheet.create({
  button: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 4,
    paddingVertical: 2,
    borderRadius: 5,
  },
  faces: { flexDirection: 'row', alignItems: 'center' },
  face: { borderWidth: 1.5, borderRadius: FACE, overflow: 'hidden' },
});
