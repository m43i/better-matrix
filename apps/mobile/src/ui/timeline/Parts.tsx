// The parts around a message's content: the message it answers, its reactions, its thread, a
// link's card and a linked GIF.

import type {
  MediaSize,
  Message,
  Quote,
  Reaction,
  ThreadSummary,
} from '@better-matrix/matrix-native/types';
import { Image } from 'expo-image';
import { router } from 'expo-router';
import { memo, useEffect, useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { fetchGif, fetchLinkPreview } from '../../state/actions/core';
import { react } from '../../state/actions/timeline';
import { useAppStore, useStore } from '../../state/react';
import { Lru } from '../../state/lru';
import { previewKey } from '../../state/reducer';
import { lastReplyText, quoteText, repliesText } from '../../state/timelineText';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Icon } from '../icons';
import { useMedia } from '../media';
import { useNow } from '../shell/useNow';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { useContentWidth } from './readersLayout';
import { openLink } from './open';

/** The message a reply answers: who wrote it and an excerpt, behind a bar. Pressing it goes there. */
export function QuoteBlock({ quote }: { quote: Quote }) {
  const palette = usePalette();
  const { scrollTo } = useTimelineContext();

  return (
    <Pressable
      onPress={() => scrollTo(quote.event_id)}
      accessibilityRole="button"
      style={styles.quote}
    >
      <View style={[styles.quoteBar, { backgroundColor: palette.textMuted }]} />
      <View style={styles.quoteText}>
        {quote.sender_name ? (
          <Text style={[font.meta, styles.quoteSender, { color: palette.text }]} numberOfLines={1}>
            {quote.sender_name}
          </Text>
        ) : null}
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={2}>
          {quoteText(quote)}
        </Text>
      </View>
    </Pressable>
  );
}

/** Reaction chips, outlined when the user's own (pressing one toggles it), and one to add more. */
export const Reactions = memo(function Reactions({
  message,
  reactions,
}: {
  message: Message;
  reactions: Reaction[];
}) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, timelineKey, thread } = useTimelineContext();

  return (
    <View style={styles.chips}>
      {reactions.map((reaction) => (
        <Pressable
          key={reaction.key}
          onPress={() => react(store, account, timelineKey, message.event_id, reaction.key)}
          accessibilityRole="button"
          accessibilityState={{ selected: reaction.mine }}
          accessibilityLabel={`${reaction.key} ${reaction.count}`}
          style={[
            styles.chip,
            {
              backgroundColor: reaction.mine ? `${palette.accent}26` : palette.surface,
              borderColor: reaction.mine ? palette.accent : palette.divider,
            },
          ]}
        >
          <Text style={[font.body, styles.chipKey]} numberOfLines={1}>
            {reaction.key}
          </Text>
          <Text style={[font.meta, { color: reaction.mine ? palette.accent : palette.textMuted }]}>
            {reaction.count}
          </Text>
        </Pressable>
      ))}
      <Pressable
        onPress={() =>
          router.push(
            routes.emoji(
              account,
              timelineKey.room_id,
              thread ? { eventId: message.event_id, thread } : { eventId: message.event_id },
            ),
          )
        }
        accessibilityRole="button"
        accessibilityLabel={t('actions.react')}
        style={[styles.chip, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      >
        <Icon name="addReaction" size={16} color={palette.textMuted} />
      </Pressable>
    </View>
  );
});

/** The latest replier's face, "3 replies" and "Last reply today at 09:51", opening the thread. */
export function ThreadLink({ root, thread }: { root: string; thread: ThreadSummary }) {
  const now = useNow();
  const palette = usePalette();
  const { account, timelineKey } = useTimelineContext();

  return (
    <Pressable
      onPress={() => router.push(routes.thread(account, timelineKey.room_id, root))}
      accessibilityRole="button"
      accessibilityHint={t('mobile.conversation.open_thread')}
      style={styles.thread}
    >
      {thread.latest ? (
        <Avatar
          account={account}
          id={thread.latest.id}
          name={thread.latest.name}
          media={thread.latest.avatar}
          size={22}
        />
      ) : (
        <Icon name="thread" size={18} color={palette.accent} />
      )}
      <Text style={[font.bodyStrong, { color: palette.accent }]}>
        {repliesText(thread.replies)}
      </Text>
      {thread.latest_at !== null ? (
        <Text style={[font.meta, styles.shrink, { color: palette.textMuted }]} numberOfLines={1}>
          {lastReplyText(thread.latest_at, now)}
        </Text>
      ) : null}
    </Pressable>
  );
}

/** The size link cards fetch their image at. */
const CARD_IMAGE: MediaSize = { type: 'Thumbnail', data: { width: 192, height: 192 } };

const CARD_IMAGE_SIDE = 64;

/** The least width a link card keeps for its words beside its picture. */
const CARD_WORDS_MIN = 100;

/**
 * A link's card once the homeserver described the page: site, title, description and image
 * (left out where it would leave the words too little room).
 */
export function LinkCard({ url }: { url: string }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account } = useTimelineContext();
  const preview = useStore((s) => s.linkPreviews[previewKey(account, url)]);
  useEffect(() => fetchLinkPreview(store, account, url), [store, account, url]);
  const image = useMedia(account, preview?.image?.[0] ?? null, CARD_IMAGE);
  // The picture shows only where it leaves the words a readable width beside it.
  const words = useContentWidth() - 2 * 12 - 3 - 2 * 12 - CARD_IMAGE_SIDE;
  const showImage = preview?.image != null && words >= CARD_WORDS_MIN;

  if (!preview) return null;

  return (
    <Pressable
      onPress={() => openLink(store, url, account)}
      accessibilityRole="link"
      accessibilityLabel={preview.title ?? url}
      android_ripple={{ color: palette.hover }}
      style={[styles.card, { backgroundColor: palette.surface, borderColor: palette.divider }]}
    >
      <View style={[styles.cardBar, { backgroundColor: palette.accent }]} />
      <View style={styles.cardWords}>
        {preview.site ? (
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {preview.site}
          </Text>
        ) : null}
        {preview.title ? (
          <Text style={[font.bodyStrong, { color: palette.accent }]} numberOfLines={2}>
            {preview.title}
          </Text>
        ) : null}
        {preview.description ? (
          <Text style={[font.meta, { color: palette.text }]} numberOfLines={3}>
            {preview.description}
          </Text>
        ) : null}
      </View>
      {showImage ? (
        image ? (
          <Image source={{ uri: image }} style={styles.cardImage} contentFit="cover" />
        ) : (
          <View style={[styles.cardImage, styles.center, { backgroundColor: palette.hover }]}>
            <Icon name="image" size={20} color={palette.textMuted} />
          </View>
        )
      ) : null}
    </Pressable>
  );
}

const GIF_WIDTH = 240;

/** Until a GIF has loaded once, it is drawn 4:3. */
const GIF_ASPECT = 4 / 3;

/** Each GIF's shape once loaded, so it is drawn right at once when shown again. */
const gifAspects = new Lru<string, number>(200);

/** A GIF behind a link, playing once fetched, as wide as fits; pressing it opens the link. */
export function LinkedGif({ url }: { url: string }) {
  const palette = usePalette();
  const store = useAppStore();
  const path = useStore((s) => s.gifs[url]);
  // The shape this view measured, for its URL (a recycled row may have measured another's).
  const [measured, setMeasured] = useState<{ url: string; aspect: number } | null>(null);
  const aspect = measured?.url === url ? measured.aspect : (gifAspects.get(url) ?? GIF_ASPECT);
  const width = Math.min(GIF_WIDTH, useContentWidth());
  useEffect(() => fetchGif(store, url), [store, url]);

  return (
    <Pressable
      onPress={() => openLink(store, url)}
      accessibilityRole="imagebutton"
      accessibilityLabel={url}
      style={styles.gif}
    >
      {path ? (
        <Image
          source={{ uri: `file://${path}` }}
          style={[styles.gifImage, { width, aspectRatio: aspect }]}
          contentFit="cover"
          autoplay
          onLoad={(event) => {
            const { width, height } = event.source;

            if (width <= 0 || height <= 0) return;
            gifAspects.set(url, width / height);
            setMeasured({ url, aspect: width / height });
          }}
        />
      ) : (
        <View
          style={[
            styles.gifImage,
            styles.center,
            {
              width,
              aspectRatio: aspect,
              backgroundColor: palette.surface,
              borderColor: palette.divider,
              borderWidth: 1,
            },
          ]}
        >
          <Text style={[font.section, { color: palette.textMuted }]}>
            {t('mobile.conversation.gif')}
          </Text>
        </View>
      )}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  quote: { flexDirection: 'row', gap: 8, maxWidth: 560, marginBottom: 2 },
  quoteBar: { width: 3, borderRadius: 2, opacity: 0.5 },
  quoteText: { flexShrink: 1, paddingVertical: 2 },
  quoteSender: { fontWeight: '600' },
  chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 6, paddingTop: 4 },
  chipKey: { flexShrink: 1 },
  // A long reaction (a shortcode, say) is cut short within the message's width; its
  // accessibility label keeps all of it.
  chip: {
    height: 28,
    minWidth: 40,
    maxWidth: '100%',
    paddingHorizontal: 8,
    gap: 6,
    borderRadius: 14,
    borderWidth: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
  },
  thread: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginTop: 4,
    paddingVertical: 3,
    alignSelf: 'flex-start',
  },
  shrink: { flexShrink: 1 },
  card: {
    marginTop: 4,
    maxWidth: 460,
    flexDirection: 'row',
    gap: 12,
    padding: 12,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
  cardBar: { width: 3, borderRadius: 1.5 },
  cardWords: { flex: 1, gap: 2 },
  cardImage: { width: CARD_IMAGE_SIDE, height: CARD_IMAGE_SIDE, borderRadius: 8 },
  center: { alignItems: 'center', justifyContent: 'center' },
  gif: { marginTop: 4, alignSelf: 'flex-start' },
  gifImage: { maxHeight: 320, borderRadius: 8, overflow: 'hidden' },
});
