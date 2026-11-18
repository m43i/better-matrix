// Who has read up to a message, as the desktop shows it in a narrow window: up to two faces and
// how many more, where `readersLayout.ts` places it. A tap lists them all (`app/readers.tsx`).

import type { Message, Person } from '@better-matrix/matrix-native/types';
import { useRouter } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { tp } from '../../i18n';
import { routes } from '../../routes';
import { READER_FACES, readerOf, readerOverflow, samePeople } from '../../state/readers';
import { useStore } from '../../state/react';
import { account as accountOf } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Touchable } from '../components';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { FACE, FACE_BORDER, FACE_OVERLAP, READERS_PADDING_Y } from './readersLayout';

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
    samePeople,
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
        <Text
          style={[font.meta, { color: palette.textMuted }]}
          numberOfLines={1}
          maxFontSizeMultiplier={1}
        >
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
    paddingVertical: READERS_PADDING_Y,
    borderRadius: 5,
  },
  faces: { flexDirection: 'row', alignItems: 'center' },
  face: { borderWidth: FACE_BORDER, borderRadius: FACE, overflow: 'hidden' },
});
