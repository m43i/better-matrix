// One message, as Slack's mobile app lays it out: the sender's face, name (in their identity
// colour) and time on the first of a run, then what it answers, its content, a linked GIF or a
// link's card, reactions and its thread. A long press opens its actions.

import type { Message } from '@better-matrix/matrix-native/types';
import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { cardLink, prepare } from '../../state/messages';
import { useStore } from '../../state/react';
import { room as roomOf } from '../../state/selectors';
import { Icon } from '../icons';
import { clockTime } from '../../state/timelineText';
import { font, identityColor } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { usePalette } from '../theme';
import { AttachmentView } from './Attachments';
import { CallCardView } from './CallCard';
import { useTimelineContext } from './context';
import { LinkCard, LinkedGif, QuoteBlock, Reactions, ThreadLink } from './Parts';
import { PollCard } from './PollCard';
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
  const { account, timelineKey, thread, openActions } = useTimelineContext();
  const showGifs = useStore((s) => s.settings.showLinkGifs);
  const showPreviews = useStore((s) => s.settings.showLinkPreviews);

  const pinned = useStore(
    (s) => roomOf(s, account, timelineKey.room_id)?.pinned.includes(message.event_id) ?? false,
  );

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
              <Text style={[font.meta, { color: palette.textMuted }]}>{status}</Text>
            ) : null}
          </View>
        )}
        {message.reply_to ? <QuoteBlock quote={message.reply_to} /> : null}
        {gif ? (
          <>
            {gif.rest ? <RichText blocks={gif.rest} /> : null}
            <LinkedGif key={gif.url} url={gif.url} />
          </>
        ) : (
          <Body message={message} latestCall={latestCall} />
        )}
        {link ? <LinkCard key={link} url={link} /> : null}
        {message.reactions.length > 0 ? (
          <Reactions message={message} reactions={message.reactions} />
        ) : null}
        {message.thread && thread === null ? (
          <ThreadLink root={message.event_id} thread={message.thread} />
        ) : null}
        {continues && status ? (
          <Text style={[font.meta, { color: palette.textMuted }]}>{status}</Text>
        ) : null}
      </View>
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
  meta: { flexDirection: 'row', alignItems: 'baseline', gap: 8 },
  name: { flexShrink: 1 },
  notice: { fontStyle: 'italic' },
});
