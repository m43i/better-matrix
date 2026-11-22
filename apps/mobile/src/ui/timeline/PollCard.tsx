// A poll in the timeline: press an answer to vote (multiple choice up to its limit, pressing
// one's own answer takes it back); counts show as people vote when disclosed, else once it
// ends. Its sender can end it, after confirming.

import type { AccountId, Message, Poll, TimelineKey } from '@better-matrix/matrix-native/types';
import { Alert, Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { endPoll, vote } from '../../state/actions/timeline';
import { isLocal, pollShowsResults, pollStatus, pollWinners } from '../../state/messages';
import { useAppStore } from '../../state/react';
import type { Store } from '../../state/store';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';

/** Asks before ending a poll: it can't be opened again. */
export function confirmEndPoll(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  pollId: string,
  question: string,
  after?: () => void,
) {
  Alert.alert(t('confirm.end_poll_title'), t('confirm.end_poll_body', { question }), [
    { text: t('common.cancel'), style: 'cancel' },
    {
      text: t('poll.end'),
      style: 'destructive',
      onPress: () => {
        endPoll(store, account, key, pollId);
        after?.();
      },
    },
  ]);
}

export function PollCard({ message, poll }: { message: Message; poll: Poll }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const results = pollShowsResults(poll);
  const winners = pollWinners(poll);
  const total = poll.answers.reduce((sum, a) => sum + a.votes, 0);
  // A local echo has no event ID to vote on yet.
  const votable = !poll.ended && !isLocal(message);
  const multiple = poll.max_selections > 1;

  return (
    <View style={[styles.card, { backgroundColor: palette.surface, borderColor: palette.divider }]}>
      <View style={styles.title}>
        <Icon name="poll" size={18} color={palette.accent} />
        <Text style={[font.bodyStrong, styles.grow, { color: palette.text }]}>{poll.question}</Text>
      </View>
      {poll.answers.map((answer) => {
        const won = winners.has(answer.id);
        const share = total > 0 ? answer.votes / total : 0;

        return (
          <Pressable
            key={answer.id}
            disabled={!votable}
            onPress={() => vote(store, account, timelineKey, message.event_id, poll, answer.id)}
            accessibilityRole={multiple ? 'checkbox' : 'radio'}
            accessibilityState={{ checked: answer.mine, disabled: !votable }}
            accessibilityLabel={answer.text}
            android_ripple={{ color: palette.hover }}
            style={[styles.answer, { borderColor: answer.mine ? palette.accent : palette.divider }]}
          >
            <View style={styles.answerHead}>
              <View
                style={[
                  styles.mark,
                  {
                    borderRadius: multiple ? 4 : 9,
                    borderColor: answer.mine ? palette.accent : palette.textMuted,
                  },
                ]}
              >
                {answer.mine ? <Icon name="check" size={12} color={palette.accent} /> : null}
              </View>
              <Text
                style={[won ? font.bodyStrong : font.body, styles.grow, { color: palette.text }]}
              >
                {answer.text}
              </Text>
              {results ? (
                <Text style={[font.meta, { color: palette.textMuted }]}>{answer.votes}</Text>
              ) : null}
            </View>
            {results ? (
              <View style={[styles.bar, { backgroundColor: palette.divider }]}>
                <View
                  style={{
                    width: `${share * 100}%`,
                    height: '100%',
                    backgroundColor: won ? palette.success : palette.accent,
                  }}
                />
              </View>
            ) : null}
          </Pressable>
        );
      })}
      <View style={styles.footer}>
        <Text style={[font.meta, styles.status, { color: palette.textMuted }]}>
          {pollStatus(poll)}
        </Text>
        {votable && message.sender_id === account ? (
          <Pressable
            onPress={() =>
              confirmEndPoll(store, account, timelineKey, message.event_id, poll.question)
            }
            accessibilityRole="button"
            hitSlop={8}
          >
            <Text style={[font.meta, { color: palette.accent }]}>{t('poll.end')}</Text>
          </Pressable>
        ) : null}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    marginTop: 4,
    // As wide as the message's column allows, whether it stretches its children or not.
    width: '100%',
    maxWidth: 460,
    padding: 14,
    gap: 10,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
  },
  title: { flexDirection: 'row', gap: 8 },
  grow: { flex: 1 },
  answer: {
    paddingHorizontal: 10,
    paddingVertical: 8,
    gap: 6,
    borderRadius: 8,
    borderWidth: 1,
    overflow: 'hidden',
  },
  answerHead: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  mark: { width: 18, height: 18, borderWidth: 2, alignItems: 'center', justifyContent: 'center' },
  bar: { height: 4, borderRadius: 2, overflow: 'hidden' },
  // On a narrow screen, ending the poll takes a line under its status.
  footer: { flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 10 },
  status: { flexGrow: 1, flexShrink: 1, minWidth: 100 },
});
