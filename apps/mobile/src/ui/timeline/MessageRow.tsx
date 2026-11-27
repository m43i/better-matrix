// One message, in the layout the settings choose. Classic rows, as Slack's mobile app lays them
// out: the sender's face, name (in their identity colour) and time on the first of a run, then
// what it answers, its content, a linked GIF or a link's card, reactions, its thread and who has
// read up to it. Chat bubbles: other people's messages at the left beside their face, the user's
// own at the right, with reactions and the thread under the bubble. A long press opens its
// actions. A message that mentions the user stands out with a warm tint and bar, as Slack's do;
// its "(edited)" opens the edit history.

import type { Message } from '@better-matrix/matrix-native/types';
import { usePersonName } from '../displayNames';
import { router } from 'expo-router';
import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { cardLink, isLocal, marksMention, prepare } from '../../state/messages';
import { routes } from '../../routes';
import { useStore } from '../../state/react';
import { Icon } from '../icons';
import { clockTime } from '../../state/timelineText';
import { font, identityColor } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { InBubble, usePalette } from '../theme';
import { AttachmentView } from './Attachments';
import { CallCardView } from './CallCard';
import { LocationCard } from './LocationCard';
import { MessageSideContext, usePinned, useTimelineContext } from './context';
import { LinkCard, LinkedGif, QuoteBlock, Reactions, ThreadLink } from './Parts';
import { PollCard } from './PollCard';
import { ReadIndicator } from './ReadIndicator';
import {
  ContentWidthContext,
  READERS_COLUMN,
  READERS_HEIGHT,
  useContentWidth,
  useReadersColumn,
} from './readersLayout';
import { RichText } from './RichText';

export const AVATAR = 36;

/** The least height of a message's sender column, its screen readers' control on every row. */
export const GUTTER_MIN = 20;

/** What a screen reader can do with a message: open its actions, by activating or long-pressing. */
const ROW_ACTIONS = [{ name: 'activate' }, { name: 'longpress' }];

/**
 * The screen-reader control that opens a message's actions, on a view that holds no other
 * control (a classic row's sender column, a bubble's time): the row itself is none (ADR 0011).
 */
function actionsControl(message: Message, sender: string, open: (message: Message) => void) {
  return {
    accessible: true,
    accessibilityRole: 'button' as const,
    accessibilityLabel: `${sender}, ${clockTime(message.timestamp_ms)}`,
    accessibilityHint: t('actions.more'),
    accessibilityActions: ROW_ACTIONS,
    onAccessibilityAction: () => open(message),
    onAccessibilityTap: () => open(message),
  };
}

/** The most of the row a bubble of words takes. */
const BUBBLE_SHARE = 0.85;

/** A bubble's padding beside its words. */
const BUBBLE_PAD_X = 12;

/** The gap between a row's picture and its message. */
const ROW_GAP = 10;

/** The kinds of content a chat bubble holds: words. */
const BUBBLED: ReadonlySet<Message['content']['type']> = new Set([
  'Text',
  'Redacted',
  'Undecryptable',
]);

export interface MessageRowProps {
  message: Message;
  /** Continues the sender's run: no face or name. */
  continues: boolean;
  /** The newest call card of the timeline. */
  latestCall: boolean;
  highlighted: boolean;
}

export const MessageRow = memo(function MessageRow(props: MessageRowProps) {
  const layout = useStore((s) => s.settings.messageLayout);

  return layout === 'bubbles' ? <BubbleRow {...props} /> : <ClassicRow {...props} />;
});

function ClassicRow({ message, continues, latestCall, highlighted }: MessageRowProps) {
  const palette = usePalette();
  const { account, openActions } = useTimelineContext();
  const pinned = usePinned(message.event_id);
  const sender = usePersonName(message.sender_name);
  const shown = useShown(message);

  const statusLine =
    message.edited || message.pending ? (
      <StatusLine message={message} color={palette.textMuted} />
    ) : null;

  return (
    // The row is no control of its own for screen readers, so the links, Show and other controls
    // in it stay reachable; its sender's column (below) opens the message's actions instead.
    <Pressable
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      android_ripple={{ color: palette.hover }}
      // Not a control on either platform: Android's TalkBack and focus skip it, its children
      // stay exposed ("no", not "no-hide-descendants").
      accessible={false}
      focusable={false}
      importantForAccessibility="no"
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
        marksMention(message, account)
          ? {
              borderLeftWidth: 3,
              borderLeftColor: palette.warning,
              paddingLeft: 13,
              backgroundColor: `${palette.warning}1A`,
            }
          : null,
        highlighted ? { backgroundColor: `${palette.accent}26` } : null,
      ]}
    >
      <View style={styles.gutter} {...actionsControl(message, sender, openActions)}>
        {continues ? null : (
          <Avatar
            account={account}
            id={message.sender_id}
            name={sender}
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
              {sender}
            </Text>
            <Text style={[font.meta, { color: palette.textMuted }]}>
              {clockTime(message.timestamp_ms)}
            </Text>
            {statusLine}
          </View>
        )}
        <Said message={message} latestCall={latestCall} shown={shown} />
        <Hanging message={message} />
        {continues ? statusLine : null}
        <ReadLine message={message} />
      </View>
      <ReadersCell message={message} />
    </Pressable>
  );
}

/** What a message shows, whatever its layout: the GIF behind a link and the link with a card. */
function useShown(message: Message) {
  const showGifs = useStore((s) => s.settings.showLinkGifs);
  const showPreviews = useStore((s) => s.settings.showLinkPreviews);
  const item = prepare(message);

  return {
    gif: showGifs && item.gif ? item.gif : null,
    link: showPreviews ? cardLink(item, showGifs) : null,
  };
}

/**
 * What a message says, in either layout: the quote it answers, its content (or a linked GIF
 * under the rest of its words) and a link's card.
 */
function Said({
  message,
  latestCall,
  shown: { gif, link },
}: {
  message: Message;
  latestCall: boolean;
  shown: ReturnType<typeof useShown>;
}) {
  return (
    <>
      {message.reply_to ? <QuoteBlock quote={message.reply_to} /> : null}
      {gif ? (
        <>
          {gif.rest ? (
            <RichText blocks={gif.rest} owner={message.event_id} media={message} />
          ) : null}
          <LinkedGif key={gif.url} url={gif.url} message={message} />
        </>
      ) : (
        <Body message={message} latestCall={latestCall} />
      )}
      {link ? <LinkCard key={link} url={link} message={message} /> : null}
    </>
  );
}

/** What hangs off a message, in either layout: its reactions and, in a room, its thread. */
function Hanging({ message }: { message: Message }) {
  const { thread } = useTimelineContext();

  return (
    <>
      {message.reactions.length > 0 ? (
        <Reactions message={message} reactions={message.reactions} />
      ) : null}
      {message.thread && thread === null ? (
        <ThreadLink root={message.event_id} thread={message.thread} />
      ) : null}
    </>
  );
}

function isRead(message: Message): boolean {
  return !message.pending && (message.read_receipts?.length ?? 0) > 0;
}

/**
 * A room message's read indicator on a line of its own, on a screen too narrow for the readers'
 * column; at the right whichever side the message keeps to.
 */
function ReadLine({ message }: { message: Message }) {
  const { thread } = useTimelineContext();
  const readersColumn = useReadersColumn();

  return thread === null && !readersColumn && isRead(message) ? (
    <View style={[styles.readersLine, styles.stretch]}>
      <ReadIndicator message={message} />
    </View>
  ) : null;
}

/** The readers' column, kept in every room message, filled or not. */
function ReadersCell({ message }: { message: Message }) {
  return useReadersColumn() ? (
    <View style={styles.readers}>
      {isRead(message) ? <ReadIndicator message={message} /> : null}
    </View>
  ) : null;
}

/**
 * "(edited)", which opens the message's edit history, and "sending", when either applies; then
 * `time` when given (a bubble's).
 */
function StatusLine({ message, color, time }: { message: Message; color: string; time?: string }) {
  const { account, timelineKey } = useTimelineContext();
  const status = message.edited || message.pending;

  return (
    <Text style={[font.meta, styles.status, { color }]} numberOfLines={1}>
      {message.edited ? (
        <Text
          accessibilityRole="button"
          accessibilityHint={t('mobile.conversation.edit_history_hint')}
          onPress={
            isLocal(message)
              ? undefined
              : () =>
                  router.push(routes.editHistory(account, timelineKey.room_id, message.event_id))
          }
          testID="message-edited"
        >
          {t('message.edited')}
        </Text>
      ) : null}
      {message.edited && message.pending ? ' · ' : null}
      {message.pending ? t('message.sending') : null}
      {time ? `${status ? ' · ' : ''}${time}` : null}
    </Text>
  );
}

function BubbleRow({ message, continues, latestCall, highlighted }: MessageRowProps) {
  const palette = usePalette();
  const { account, unnamed, openActions } = useTimelineContext();
  const pinned = usePinned(message.event_id);
  const sender = usePersonName(message.sender_name);
  const shown = useShown(message);
  const own = message.sender_id === account;
  const named = !own && !continues && !unnamed.has(message.sender_id);
  // Someone else's message that mentions the user keeps a warm mark, as a classic row does: a
  // tint on the row and, on a bubble of words, a warm outline.
  const mentions = marksMention(message, account);
  // Words get a bubble; media and cards bring their own frames.
  const bubbled = shown.gif === null && BUBBLED.has(message.content.type);
  // What a bubble leaves its words, for what sizes itself to them (a link's card): a share of
  // its column, which the user's own rows widen by the picture's gutter they leave out.
  const contentWidth = useContentWidth();
  const column = contentWidth + (own ? AVATAR + ROW_GAP : 0);
  const bubbleWidth = Math.floor(column * BUBBLE_SHARE) - 2 * BUBBLE_PAD_X;

  // What the message says: in a bubble, its muted words take the bubble's shade.
  const said = <Said message={message} latestCall={latestCall} shown={shown} />;

  return (
    // As a classic row: no control of its own for screen readers; its time opens the actions.
    <Pressable
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      android_ripple={{ color: palette.hover }}
      accessible={false}
      focusable={false}
      importantForAccessibility="no"
      style={[
        styles.row,
        { paddingTop: continues ? 2 : 10, opacity: message.pending ? 0.6 : 1 },
        mentions ? { backgroundColor: `${palette.warning}1A` } : null,
        highlighted ? { backgroundColor: `${palette.accent}26` } : null,
      ]}
    >
      {own ? null : (
        <View style={styles.gutter}>
          {continues ? null : (
            <Avatar
              account={account}
              id={message.sender_id}
              name={sender}
              media={message.sender_avatar}
              size={AVATAR}
            />
          )}
        </View>
      )}
      <MessageSideContext.Provider value={own ? 'flex-end' : 'flex-start'}>
        <View style={[styles.bubbleColumn, { alignItems: own ? 'flex-end' : 'flex-start' }]}>
          {pinned ? (
            <View style={[styles.meta, { gap: 4 }]}>
              <Icon name="pin" size={12} color={palette.accent} />
              <Text style={[font.meta, { color: palette.accent }]}>{t('message.pinned')}</Text>
            </View>
          ) : null}
          <View
            testID={`bubble-${message.event_id}`}
            style={[
              styles.bubble,
              bubbled
                ? [
                    styles.filled,
                    { backgroundColor: own ? palette.bubbleOwn : palette.bubble },
                    mentions ? [styles.mentioned, { borderColor: palette.warning }] : null,
                  ]
                : // Cards and media take the width a classic row gives them, which they are sized
                  // for: a poll fills it, a picture keeps its size on the message's side.
                  // No wider than a classic row's content, though own rows leave out the picture.
                  [
                    styles.unfilled,
                    { maxWidth: contentWidth, alignItems: own ? 'flex-end' : 'flex-start' },
                  ],
            ]}
          >
            {named ? (
              <Text
                style={[font.section, styles.name, { color: identityColor(message.sender_id) }]}
                numberOfLines={1}
              >
                {sender}
              </Text>
            ) : null}
            {bubbled ? (
              <InBubble>
                <ContentWidthContext.Provider value={bubbleWidth}>
                  {said}
                </ContentWidthContext.Provider>
              </InBubble>
            ) : (
              said
            )}
            {/* A view, not the text, carries the control: iOS doesn't activate a text's. */}
            <View
              style={bubbled ? styles.time : null}
              {...actionsControl(message, sender, openActions)}
            >
              <StatusLine
                message={message}
                color={bubbled ? palette.bubbleMeta : palette.textMuted}
                time={clockTime(message.timestamp_ms)}
              />
            </View>
          </View>
          <Hanging message={message} />
          <ReadLine message={message} />
        </View>
      </MessageSideContext.Provider>
      <ReadersCell message={message} />
    </Pressable>
  );
}

function Body({ message, latestCall }: { message: Message; latestCall: boolean }) {
  const sender = usePersonName(message.sender_name);
  const palette = usePalette();
  const content = message.content;

  switch (content.type) {
    case 'Text':
    case 'Emote': {
      const blocks = prepare(message, sender).blocks;

      return blocks ? <RichText blocks={blocks} owner={message.event_id} media={message} /> : null;
    }

    case 'Location':
      return <LocationCard message={message} location={content.data} />;

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
    gap: ROW_GAP,
    paddingHorizontal: 16,
    paddingBottom: 2,
  },
  // Stretched to the row, so a following message's empty column still has bounds to focus.
  gutter: { width: AVATAR, alignSelf: 'stretch', minHeight: GUTTER_MIN },
  content: { flex: 1, minWidth: 0, gap: 2 },
  bubbleColumn: { flex: 1, minWidth: 0, gap: 4 },
  bubble: { minWidth: 0, gap: 4 },
  filled: {
    maxWidth: `${BUBBLE_SHARE * 100}%`,
    borderRadius: 16,
    paddingHorizontal: BUBBLE_PAD_X,
    paddingVertical: 7,
  },
  // A bubble mentioning the user: a warm outline, inside its padding so its words don't move.
  mentioned: {
    borderWidth: 1.5,
    paddingHorizontal: BUBBLE_PAD_X - 1.5,
    paddingVertical: 7 - 1.5,
  },
  // As wide as a classic row's content, which media are sized for.
  unfilled: { width: '100%' },
  time: { alignSelf: 'flex-end' },
  // Kept in every room message, filled or not, so a receipt never rewraps or grows it; on a
  // screen too narrow for it, the indicator takes a line under the message instead.
  readersLine: { flexDirection: 'row', justifyContent: 'flex-end' },
  stretch: { alignSelf: 'stretch' },
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
