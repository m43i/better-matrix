// A message's attachment: an image sized by the sender's dimensions (so the list doesn't jump
// when it loads), a video's poster, an audio player, or a file card that shares the file.

import type { Attachment, Message } from '@better-matrix/matrix-native/types';
import { useAudioPlayer, useAudioPlayerStatus } from 'expo-audio';
import { Image } from 'expo-image';
import { router } from 'expo-router';
import { type ReactNode, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  useWindowDimensions,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { fetchOriginal } from '../../state/actions/timeline';
import { attachmentPreview, playTime, previewSize } from '../../state/messages';
import { useAppStore } from '../../state/react';
import { humanSize } from '../../state/timelineText';
import { addNotice } from '../../state/reducer';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Icon, type IconName } from '../icons';
import { ORIGINAL, PREVIEW_SIZE, useMedia } from '../media';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { READERS_COLUMN } from './ReadIndicator';
import { shareAttachment } from './files';

/** Room beside a message's content: the row's padding, the avatar and its gap. */
const ROW_INSET = 16 + 36 + 10 + 16;

/** Room a room message keeps for its read indicator, with its gap. */
const READERS_INSET = READERS_COLUMN + 10;

/**
 * The largest box an image, video or GIF takes: the content's width (clear of the readers'
 * column in a room), up to the desktop's 360 × 280.
 */
export function useMediaBox() {
  const { width } = useWindowDimensions();
  const { thread } = useTimelineContext();
  const inset = ROW_INSET + (thread === null ? READERS_INSET : 0);

  return { width: Math.min(width - inset, 360), height: 280 };
}

export function AttachmentView({
  message,
  attachment,
}: {
  message: Message;
  attachment: Attachment;
}) {
  switch (attachment.kind) {
    case 'Image':
      return attachmentPreview(attachment) ? (
        <ImageAttachment message={message} attachment={attachment} />
      ) : (
        <FileCard message={message} attachment={attachment} />
      );
    case 'Video':
      return attachment.media ? (
        <VideoAttachment message={message} attachment={attachment} />
      ) : (
        <FileCard message={message} attachment={attachment} />
      );
    case 'Audio':
      return attachment.media ? (
        <AudioCard key={attachment.media} message={message} attachment={attachment} />
      ) : (
        <FileCard message={message} attachment={attachment} />
      );
    case 'File':
      return <FileCard message={message} attachment={attachment} />;
  }
}

function useOpenPreview(message: Message): () => void {
  const { account, timelineKey, thread } = useTimelineContext();

  return () =>
    router.push(
      routes.preview(account, timelineKey.room_id, message.event_id, thread ?? undefined),
    );
}

function ImageAttachment({ message, attachment }: { message: Message; attachment: Attachment }) {
  const palette = usePalette();
  const { account, openActions } = useTimelineContext();
  const preview = attachmentPreview(attachment);
  const uri = useMedia(account, preview?.media ?? null, preview?.size ?? PREVIEW_SIZE);
  const size = previewSize(attachment.dimensions, useMediaBox());
  const open = useOpenPreview(message);

  return (
    <Pressable
      onPress={open}
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      accessibilityRole="imagebutton"
      accessibilityLabel={attachment.name}
      style={styles.media}
    >
      {uri ? (
        <Image
          source={{ uri }}
          style={[size, styles.rounded]}
          contentFit="cover"
          transition={120}
          recyclingKey={uri}
        />
      ) : (
        <View
          style={[
            size,
            styles.rounded,
            styles.placeholder,
            { backgroundColor: palette.surface, borderColor: palette.divider },
          ]}
        >
          <Icon name="image" size={24} color={palette.textMuted} />
        </View>
      )}
    </Pressable>
  );
}

/** A video's poster (the sender's thumbnail) with a play button; it plays in the preview. */
function VideoAttachment({ message, attachment }: { message: Message; attachment: Attachment }) {
  const palette = usePalette();
  const { account, openActions } = useTimelineContext();
  const poster = useMedia(account, attachment.thumbnail, ORIGINAL);
  const size = previewSize(attachment.dimensions, useMediaBox());
  const open = useOpenPreview(message);

  return (
    <Pressable
      onPress={open}
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      accessibilityRole="button"
      accessibilityLabel={`${t('mobile.conversation.play_video')}: ${attachment.name}`}
      style={styles.media}
    >
      <View
        style={[
          size,
          styles.rounded,
          styles.placeholder,
          { backgroundColor: palette.surface, borderColor: palette.divider },
        ]}
      >
        {poster ? (
          <Image
            source={{ uri: poster }}
            style={[StyleSheet.absoluteFill, styles.rounded]}
            contentFit="cover"
          />
        ) : null}
        <View style={[styles.play, { backgroundColor: palette.scrim }]}>
          <Icon name="play" size={22} color="#ffffff" />
        </View>
      </View>
    </Pressable>
  );
}

/** A card with an icon, the name and size, pressing which runs `onPress`. */
function Card({
  icon,
  attachment,
  onPress,
  onLongPress,
  busy = false,
  detail,
  leading,
}: {
  icon: IconName;
  attachment: Attachment;
  onPress?: () => void;
  onLongPress?: () => void;
  busy?: boolean;
  detail?: string;
  leading?: ReactNode;
}) {
  const palette = usePalette();
  const sub = detail ?? (attachment.size !== null ? humanSize(attachment.size) : undefined);

  const body = (
    <>
      {leading ?? (
        <View style={[styles.cardIcon, { backgroundColor: `${palette.accent}26` }]}>
          {busy ? (
            <ActivityIndicator color={palette.accent} />
          ) : (
            <Icon name={icon} size={20} color={palette.accent} />
          )}
        </View>
      )}
      <View style={styles.cardText}>
        <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
          {attachment.name}
        </Text>
        {sub ? <Text style={[font.meta, { color: palette.textMuted }]}>{sub}</Text> : null}
      </View>
    </>
  );

  const style = [styles.card, { backgroundColor: palette.surface, borderColor: palette.divider }];

  if (!onPress && !onLongPress) return <View style={style}>{body}</View>;

  return (
    <Pressable
      onPress={onPress}
      onLongPress={onLongPress}
      delayLongPress={350}
      disabled={busy}
      accessibilityRole={onPress ? 'button' : undefined}
      accessibilityLabel={attachment.name}
      accessibilityActions={
        onLongPress ? [{ name: 'longpress', label: t('actions.more') }] : undefined
      }
      onAccessibilityAction={(event) => {
        if (event.nativeEvent.actionName === 'longpress') onLongPress?.();
      }}
      android_ripple={{ color: palette.hover }}
      style={style}
    >
      {body}
    </Pressable>
  );
}

/** A file: pressing it downloads it and opens the share sheet. */
function FileCard({ message, attachment }: { message: Message; attachment: Attachment }) {
  const store = useAppStore();
  const { account, openActions } = useTimelineContext();
  const [busy, setBusy] = useState(false);

  const icon: IconName =
    attachment.kind === 'Image'
      ? 'image'
      : attachment.kind === 'Video'
        ? 'video'
        : attachment.kind === 'Audio'
          ? 'audio'
          : 'file';

  const share = () => {
    setBusy(true);
    void shareAttachment(store, account, attachment).finally(() => setBusy(false));
  };

  return (
    <Card
      icon={icon}
      attachment={attachment}
      busy={busy}
      onPress={attachment.media ? share : undefined}
      onLongPress={() => openActions(message)}
    />
  );
}

/** Audio and voice memos: play and pause, the time played of its length, and a progress bar. */
function AudioCard({ message, attachment }: { message: Message; attachment: Attachment }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, openActions } = useTimelineContext();
  const player = useAudioPlayer(null, { updateInterval: 250 });
  const status = useAudioPlayerStatus(player);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const media = attachment.media;

  const toggle = () => {
    if (!media) return;

    if (!loaded) {
      setBusy(true);
      fetchOriginal(store, account, media)
        .then((uri) => {
          player.replace({ uri });
          setLoaded(true);
          player.play();
        })
        .catch((cause: unknown) =>
          store.update((s) => addNotice(s, t('notice.audio', { error: String(cause) }))),
        )
        .finally(() => setBusy(false));

      return;
    }

    if (status.playing) {
      player.pause();

      return;
    }

    if (status.didJustFinish || (status.duration > 0 && status.currentTime >= status.duration))
      void player.seekTo(0);
    player.play();
  };

  const progress = status.duration > 0 ? Math.min(1, status.currentTime / status.duration) : 0;

  const detail = loaded
    ? status.duration > 0
      ? `${playTime(status.currentTime)} / ${playTime(status.duration)}`
      : playTime(status.currentTime)
    : undefined;

  const button = (
    <Pressable
      onPress={toggle}
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      disabled={busy}
      accessibilityRole="button"
      accessibilityLabel={status.playing ? t('audio.pause') : t('audio.play')}
      hitSlop={6}
      style={[styles.playButton, { backgroundColor: palette.accent }]}
    >
      {busy ? (
        <ActivityIndicator color={palette.onAccent} />
      ) : (
        <Icon name={status.playing ? 'pause' : 'play'} size={18} color={palette.onAccent} />
      )}
    </Pressable>
  );

  return (
    <View>
      <Card
        icon="audio"
        attachment={attachment}
        leading={button}
        detail={detail}
        onLongPress={() => openActions(message)}
      />
      {loaded ? (
        <View style={[styles.progress, { backgroundColor: palette.divider }]}>
          <View
            style={{ width: `${progress * 100}%`, height: '100%', backgroundColor: palette.accent }}
          />
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  media: { marginTop: 4, alignSelf: 'flex-start' },
  rounded: { borderRadius: 8, overflow: 'hidden' },
  placeholder: {
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: StyleSheet.hairlineWidth,
  },
  play: { width: 48, height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center' },
  card: {
    marginTop: 4,
    maxWidth: 420,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 12,
    paddingVertical: 10,
    borderRadius: 8,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
  cardIcon: {
    width: 36,
    height: 36,
    borderRadius: 6,
    alignItems: 'center',
    justifyContent: 'center',
  },
  cardText: { flexShrink: 1, gap: 2 },
  playButton: {
    width: 36,
    height: 36,
    borderRadius: 18,
    alignItems: 'center',
    justifyContent: 'center',
  },
  progress: { height: 3, borderRadius: 1.5, marginTop: 4, maxWidth: 420, overflow: 'hidden' },
});
