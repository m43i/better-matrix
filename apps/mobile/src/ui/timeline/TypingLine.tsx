// Who is typing in the room, over the composer: their faces and "Ada is typing…". The line keeps
// its height when nobody types, so the timeline doesn't jump.

import type { AccountId, Person } from '@better-matrix/matrix-native/types';
import { StyleSheet, Text, View } from 'react-native';

import { account as accountOf } from '../../state/selectors';
import { shallow, useStore } from '../../state/react';
import { typingText } from '../../state/timelineText';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { usePalette } from '../theme';

const FACES = 3;

const FACE = 16;

const NOBODY: Person[] = [];

export function TypingLine({ account, roomId }: { account: AccountId; roomId: string }) {
  const palette = usePalette();

  const people = useStore(
    (s) => (accountOf(s, account)?.typing[roomId] ?? NOBODY).filter((p) => p.id !== account),
    shallow,
  );

  const sentence = typingText(people);

  return (
    <View
      style={styles.line}
      accessibilityLiveRegion="polite"
      accessibilityLabel={sentence || undefined}
    >
      {people.length > 0 ? (
        <>
          <View style={styles.faces}>
            {people.slice(0, FACES).map((person, index) => (
              <View
                key={person.id}
                style={[styles.face, { marginLeft: index === 0 ? 0 : -5, borderColor: palette.bg }]}
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
          <Text style={[font.meta, styles.text, { color: palette.textMuted }]} numberOfLines={1}>
            {sentence}
          </Text>
        </>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  line: { height: 22, flexDirection: 'row', alignItems: 'center', gap: 6, paddingHorizontal: 16 },
  faces: { flexDirection: 'row' },
  face: { borderWidth: 1.5, borderRadius: FACE, overflow: 'hidden' },
  text: { flexShrink: 1 },
});
