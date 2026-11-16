// The rows around messages: room events (with merged runs), date dividers, the "New" marker,
// where visible history starts, a page loading or failed, and a thread's reply count.

import type { RoomEvent } from '@better-matrix/matrix-native/types';
import { memo } from 'react';
import Animated, { useAnimatedStyle, useReducedMotion, withTiming } from 'react-native-reanimated';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { retryHistory } from '../../state/actions/timeline';
import { MARKER_FADE_MS, MARKER_FADE_REDUCED_MS } from '../../state/newMarker';
import { shallow, useAppStore, useStore } from '../../state/react';
import { room as roomOf } from '../../state/selectors';
import { dateLabel, EVENT_PEOPLE, eventPeople, eventRunText, inlineName, repliesText } from '../../state/timelineText';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';

const FACE = 18;

/** One room event, or a run of joins or leaves, as a small line with the people's faces. */
export const RoomEventLine = memo(RoomEventLineView, (a, b) => shallow(a.events, b.events));

function RoomEventLineView({ events }: { events: RoomEvent[] }) {
  const palette = usePalette();
  const { account } = useTimelineContext();
  const people = eventPeople(events);
  return (
    <View style={styles.event}>
      <View style={styles.faces}>
        {people.slice(0, EVENT_PEOPLE).map((person, index) => (
          <View key={person.id} style={[styles.face, { marginLeft: index === 0 ? 0 : -6, borderColor: palette.bg }]}>
            <Avatar account={account} id={person.id} name={person.name} media={person.avatar} size={FACE} />
          </View>
        ))}
        {people.length > EVENT_PEOPLE ? (
          <View style={[styles.face, styles.more, { marginLeft: -6, borderColor: palette.bg, backgroundColor: palette.surface }]}>
            <Text style={[font.badge, { color: palette.textMuted }]}>+</Text>
          </View>
        ) : null}
      </View>
      <Text style={[font.meta, styles.grow, { color: palette.textMuted }]}>{eventRunText(events)}</Text>
    </View>
  );
}

/** A rule with a label in its middle: date dividers, the "New" marker, a thread's replies. */
export function Rule({ label, accent = false }: { label: string; accent?: boolean }) {
  const palette = usePalette();
  const color = accent ? palette.mention : palette.textMuted;
  const line = accent ? palette.mention : palette.divider;
  return (
    <View style={styles.rule} accessibilityRole="header">
      <View style={[styles.line, { backgroundColor: line }]} />
      <Text style={[font.section, { color }]}>{label}</Text>
      <View style={[styles.line, { backgroundColor: line }]} />
    </View>
  );
}

export function DateDivider({ at }: { at: number }) {
  return <Rule label={dateLabel(at, Date.now())} />;
}

/** The "New" divider; once read past it fades out in place, keeping its height. */
export function NewMarker({ fading = false }: { fading?: boolean }) {
  const reducedMotion = useReducedMotion();
  const fade = reducedMotion ? MARKER_FADE_REDUCED_MS : MARKER_FADE_MS;
  const style = useAnimatedStyle(() => ({ opacity: withTiming(fading ? 0 : 1, { duration: fading ? fade : 0 }) }));
  return (
    <Animated.View style={style}>
      <Rule label={t('timeline.new')} accent />
    </Animated.View>
  );
}

export function RepliesDivider({ count }: { count: number }) {
  return <Rule label={repliesText(count)} />;
}

/** Where the room's visible history begins: its name, and whether earlier messages are hidden. */
export function StartBlock() {
  const palette = usePalette();
  const { account, timelineKey } = useTimelineContext();
  const room = useStore((s) => roomOf(s, account, timelineKey.room_id));
  if (!room) return null;
  const hidden =
    room.history === 'Joined' ? t('timeline.start_hidden_joined') : room.history === 'Invited' ? t('timeline.start_hidden_invited') : null;
  return (
    <View style={styles.start}>
      <Avatar account={account} id={room.room_id} name={room.name} media={room.avatar} size={56} />
      <Text style={[font.title, { color: palette.text }]}>{room.kind === 'Group' ? `# ${room.name}` : room.name}</Text>
      <Text style={[font.body, { color: palette.textMuted }]}>{t('timeline.start', { room: inlineName(room) })}</Text>
      {hidden ? (
        <View style={styles.hidden}>
          <Icon name="eyeOff" size={16} color={palette.textMuted} />
          <Text style={[font.meta, styles.grow, { color: palette.textMuted }]}>{hidden}</Text>
        </View>
      ) : null}
    </View>
  );
}

export function LoadingRow() {
  const palette = usePalette();
  return (
    <View style={styles.loading}>
      <ActivityIndicator color={palette.textMuted} size="small" />
      <Text style={[font.meta, { color: palette.textMuted }]}>{t('timeline.loading')}</Text>
    </View>
  );
}

/** A page of history that failed, with Try again (the error in its accessible label). */
export function HistoryFailedRow({ error }: { error: string }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  return (
    <View style={styles.loading}>
      <Text style={[font.meta, { color: palette.textMuted }]}>{t('timeline.history_failed')}</Text>
      <Pressable
        onPress={() => retryHistory(store, account, timelineKey)}
        accessibilityRole="button"
        accessibilityLabel={t('timeline.history_failed_detail', { error })}
        hitSlop={8}
      >
        <Text style={[font.meta, { color: palette.accent, fontWeight: '600' }]}>{t('common.try_again')}</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  event: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 16, paddingVertical: 4 },
  faces: { flexDirection: 'row', alignItems: 'center' },
  face: { borderWidth: 1.5, borderRadius: FACE, overflow: 'hidden' },
  more: { width: FACE + 3, height: FACE + 3, alignItems: 'center', justifyContent: 'center' },
  grow: { flex: 1 },
  rule: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 16, paddingVertical: 10 },
  line: { flex: 1, height: StyleSheet.hairlineWidth },
  start: { paddingHorizontal: 16, paddingTop: 24, paddingBottom: 16, gap: 6 },
  hidden: { flexDirection: 'row', alignItems: 'center', gap: 8, marginTop: 6 },
  loading: { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 8, height: 44 },
});
