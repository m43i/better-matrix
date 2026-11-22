// One message, as Slack's mobile app lays it out: the sender's face, name (in their identity
// colour) and time on the first of a run, then what it answers, its content, a linked GIF or a
// link's card, reactions, its thread and who has read up to it. A long press opens its actions.

import type { Message } from '@better-matrix/matrix-native/types';
import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { cardLink, prepare } from '../../state/messages';
import { useStore } from '../../state/react';
import { Icon } from '../icons';
import { clockTime } from '../../state/timelineText';
import { font, identityColor } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { usePalette } from '../theme';
import { AttachmentView } from './Attachments';
import { CallCardView } from './CallCard';
import { usePinned, useTimelineContext } from './context';
import { LinkCard, LinkedGif, QuoteBlock, Reactions, ThreadLink } from './Parts';
import { PollCard } from './PollCard';
import { ReadIndicator } from './ReadIndicator';
import { READERS_COLUMN, READERS_HEIGHT, useReadersColumn } from './readersLayout';
import { RichText } from './RichText';

export const AVATAR = 36;

export interface MessageRowProps {
  message: Message;
  /** Continues the sender's run: no face or name. */
  continues: boolean;
  /** The newest call card of the timeline. */
  latestCall: boolean;
  highlighted: boolean;
}

export const MessageRow = memo(function MessageRow({
  message,
  continues,
  latestCall,
  highlighted,
}: MessageRowProps) {
  const palette = usePalette();
  const { account, thread, openActions } = useTimelineContext();
  const readersColumn = useReadersColumn();
  const read = !message.pending && (message.read_receipts?.length ?? 0) > 0;
  const showGifs = useStore((s) => s.settings.showLinkGifs);
  const showPreviews = useStore((s) => s.settings.showLinkPreviews);

  const pinned = usePinned(message.event_id);

  const item = prepare(message);
  const gif = showGifs && item.gif ? item.gif : null;
  const link = showPreviews ? cardLink(item, showGifs) : null;

  const status = [
    message.edited ? t('message.edited') : null,
    message.pending ? t('message.sending') : null,
  ]
    .filter((s): s is string => s !== null)
    .join(' · ');

  return (
    <Pressable
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      android_ripple={{ color: palette.hover }}
      accessibilityHint={t('actions.more')}
      style={[
        styles.row,
        { paddingTop: continues ? 2 : 10, opacity: message.pending ? 0.6 : 1 },
        pinned
          ? {
              borderLeftWidth: 3,
              borderLeftColor: palette.accent,
              paddingLeft: 13,
              backgroundColor: `${palette.accent}0F`,
            }
          : null,
        highlighted ? { backgroundColor: `${palette.accent}26` } : null,
      ]}
    >
      <View style={styles.gutter}>
        {continues ? null : (
          <Avatar
            account={account}
            id={message.sender_id}
            name={message.sender_name}
            media={message.sender_avatar}
            size={AVATAR}
          />
        )}
      </View>
      <View style={styles.content}>
        {pinned ? (
          <View style={[styles.meta, { gap: 4 }]}>
            <Icon name="pin" size={12} color={palette.accent} />
            <Text style={[font.meta, { color: palette.accent }]}>{t('message.pinned')}</Text>
          </View>
        ) : null}
        {continues ? null : (
          <View style={styles.meta}>
            <Text
              style={[font.bodyStrong, styles.name, { color: identityColor(message.sender_id) }]}
              numberOfLines={1}
            >
              {message.sender_name}
            </Text>
            <Text style={[font.meta, { color: palette.textMuted }]}>
              {clockTime(message.timestamp_ms)}
            </Text>
            {status ? (
              <Text
                style={[font.meta, styles.status, { color: palette.textMuted }]}
                numberOfLines={1}
              >
                {status}
              </Text>
            ) : null}
          </View>
        )}
        {message.reply_to ? <QuoteBlock quote={message.reply_to} /> : null}
        {gif ? (
          <>
            {gif.rest ? <RichText blocks={gif.rest} /> : null}
            <LinkedGif key={gif.url} url={gif.url} message={message} />
          </>
        ) : (
          <Body message={message} latestCall={latestCall} />
        )}
        {link ? <LinkCard key={link} url={link} message={message} /> : null}
        {message.reactions.length > 0 ? (
          <Reactions message={message} reactions={message.reactions} />
        ) : null}
        {message.thread && thread === null ? (
          <ThreadLink root={message.event_id} thread={message.thread} />
        ) : null}
        {continues && status ? (
          <Text style={[font.meta, { color: palette.textMuted }]}>{status}</Text>
        ) : null}
        {thread === null && !readersColumn && read ? (
          <View style={styles.readersLine}>
            <ReadIndicator message={message} />
          </View>
        ) : null}
      </View>
      {readersColumn ? (
        <View style={styles.readers}>{read ? <ReadIndicator message={message} /> : null}</View>
      ) : null}
    </Pressable>
  );
});

function Body({ message, latestCall }: { message: Message; latestCall: boolean }) {
  const palette = usePalette();
  const content = message.content;

  switch (content.type) {
    case 'Text': {
      const blocks = prepare(message).blocks;

      return blocks ? <RichText blocks={blocks} /> : null;
    }

    case 'Attachment':
      // Keyed by message: a recycled row starts its file's state afresh.
      return <AttachmentView key={message.event_id} message={message} attachment={content.data} />;
    case 'Redacted':
      return (
        <Text style={[font.body, styles.notice, { color: palette.textMuted }]}>
          {t('message.deleted')}
        </Text>
      );
    case 'Undecryptable':
      return (
        <Text style={[font.body, styles.notice, { color: palette.textMuted }]}>
          {t('message.undecryptable')}
        </Text>
      );
    case 'Poll':
      return <PollCard message={message} poll={content.data} />;
    case 'Call':
      return <CallCardView card={content.data} latest={latestCall} />;
  }
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 10,
    paddingHorizontal: 16,
    paddingBottom: 2,
  },
  gutter: { width: AVATAR },
  content: { flex: 1, minWidth: 0, gap: 2 },
  // Kept in every room message, filled or not, so a receipt never rewraps or grows it; on a
  // screen too narrow for it, the indicator takes a line under the message instead.
  readersLine: { flexDirection: 'row', justifyContent: 'flex-end' },
  readers: {
    width: READERS_COLUMN,
    minHeight: READERS_HEIGHT,
    alignSelf: 'flex-end',
    flexDirection: 'row',
    justifyContent: 'flex-end',
  },
  meta: { flexDirection: 'row', alignItems: 'baseline', gap: 8 },
  name: { flexShrink: 1 },
  // A long status ("Wird gesendet …") gives way before the name on a narrow screen; it keeps to
  // the line, so a message doesn't change height once it is sent.
  status: { flexShrink: 3 },
  notice: { fontStyle: 'italic' },
});
