// A call's card: what kind it is and how it stands. The newest card in the room shows whether
// the room's call is under way, with Join; older ones have ended (see `callStatus`).

import type { CallCard as Card, CallVia } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { joinCall } from '../../state/actions/calls';
import { callStatus } from '../../state/messages';
import { useAppStore, useStore } from '../../state/react';
import { room as roomOf } from '../../state/selectors';
import { callTitle } from '../../state/timelineText';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useMediaBox } from './Attachments';
import { useTimelineContext } from './context';

const PADDING = 12;

/** The width a call card's picture and words keep beside Join before Join wraps under them. */
const HEAD_MIN = 120;

export function CallCardView({ card, latest }: { card: Card; latest: boolean }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const roomId = timelineKey.room_id;
  const room = useStore((s) => roomOf(s, account, roomId));

  const callHere = useStore((s): CallVia | null => {
    const current = s.call.current;

    return current && current.account === account && current.room_id === roomId
      ? current.via
      : null;
  });

  const ringing = useStore((s) =>
    s.call.rings.some((r) => r.account === account && r.room_id === roomId),
  );

  // The picture and words keep a readable width, or all the card has.
  const headMin = Math.max(0, Math.min(HEAD_MIN, useMediaBox().width - 2 * (PADDING + 1)));

  if (!room) return null;
  const status = callStatus(card, room, { latest, callHere, ringing });

  const join = () => {
    joinCall(store, account, roomId, card.video === true);
    router.push(routes.call());
  };

  return (
    <View
      style={[
        styles.card,
        {
          backgroundColor: palette.surface,
          borderColor: status.ongoing ? `${palette.success}99` : palette.divider,
        },
      ]}
    >
      <View style={[styles.head, { minWidth: headMin }]}>
        <View
          style={[
            styles.tile,
            { backgroundColor: status.ongoing ? palette.success : palette.hover },
          ]}
        >
          <Icon
            name={card.video === true ? 'video' : 'call'}
            size={20}
            color={status.ongoing ? palette.onAccent : palette.textMuted}
          />
        </View>
        <View style={styles.words}>
          <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
            {callTitle(card)}
          </Text>
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {status.label}
          </Text>
        </View>
      </View>
      {status.joined ? (
        <Text style={[font.meta, styles.joined, { color: palette.textMuted }]}>
          {t('call.card.joined')}
        </Text>
      ) : status.join ? (
        <Pressable
          onPress={join}
          accessibilityRole="button"
          accessibilityHint={t('call.join_hint')}
          style={[styles.join, { backgroundColor: palette.success }]}
        >
          <Icon name="call" size={16} color={palette.onAccent} />
          <Text style={[font.bodyStrong, { color: palette.onAccent }]}>{t('call.join')}</Text>
        </Pressable>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    marginTop: 4,
    maxWidth: 460,
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: 12,
    padding: PADDING,
    borderRadius: 10,
    borderWidth: 1,
  },
  tile: { width: 40, height: 40, borderRadius: 10, alignItems: 'center', justifyContent: 'center' },
  // The picture and words keep a readable width; on a narrow phone, Join takes a line under
  // them.
  head: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    flexGrow: 1,
    flexShrink: 1,
  },
  words: { flex: 1, gap: 2 },
  joined: { flexShrink: 1, textAlign: 'right' },
  join: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 12,
    height: 34,
    borderRadius: 17,
  },
});
