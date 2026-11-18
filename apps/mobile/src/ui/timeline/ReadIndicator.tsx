// Who has read up to a message, as the desktop shows it in a narrow window: up to two faces and
// how many more, in a column every room message keeps free at its right (`READERS_COLUMN`), so
// a receipt never rewraps or grows its message. A tap lists them all (`app/readers.tsx`).

import type { Message, Person } from '@better-matrix/matrix-native/types';
import { useRouter } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { tp } from '../../i18n';
import { routes } from '../../routes';
import { READER_FACES, readerOf, readerOverflow } from '../../state/readers';
import { shallow, useStore } from '../../state/react';
import { account as accountOf } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Touchable } from '../components';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';

const FACE = 16;

/** The ring that separates overlapping faces; it adds to each face's size. */
const FACE_BORDER = 1.5;

const FACE_OVERLAP = 6;

const FACE_OUTER = FACE + 2 * FACE_BORDER;

/** Room for the overflow count ("+99", or "99+") after the faces. */
const COUNT = 24;

/** The width every room message keeps free at its right for its read indicator. */
export const READERS_COLUMN =
  FACE_OUTER + (READER_FACES - 1) * (FACE_OUTER - FACE_OVERLAP) + 6 + COUNT + 8;

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
  const more = readerOverflow(receipts.length);

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
      {more ? (
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
          {more}
        </Text>
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
    // No taller than a line of text, so it never makes a one-line message taller.
    paddingVertical: (font.body.lineHeight - FACE_OUTER) / 2,
    borderRadius: 5,
  },
  faces: { flexDirection: 'row', alignItems: 'center' },
  face: { borderWidth: FACE_BORDER, borderRadius: FACE, overflow: 'hidden' },
});
