// The attachment preview, full screen on black: an image to pinch, pan and double-tap to zoom,
// a video in the platform's player, or an audio clip in a player of our own; who sent it and
// when at the top, with share, save and close. When the sender posted it in a batch with other
// attachments (`state/gallery.ts`), a strip of thumbnails runs along the bottom and a swipe
// sideways steps through them, round past either end.

import type { Attachment } from '@better-matrix/matrix-native/types';
import { Image } from 'expo-image';
import { router, useLocalSearchParams } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useVideoPlayer, VideoView } from 'expo-video';
import { useCallback, useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Platform,
  Pressable,
  type StyleProp,
  StyleSheet,
  Text,
  View,
  type ViewStyle,
} from 'react-native';
import {
  GestureDetector,
  useLongPressGesture,
  usePanGesture,
  usePinchGesture,
  useSimultaneousGestures,
  useTapGesture,
} from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withTiming } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { t } from '../i18n';
import { routes } from '../routes';
import { galleryAround, showsStrip, stepGallery } from '../state/gallery';
import { attachmentPreview } from '../state/messages';
import { shallow, useAppStore, useStore } from '../state/react';
import { timeline } from '../state/selectors';
import { roomKey, threadKey } from '../state/timeline';
import { sentAt } from '../state/timelineText';
import { font } from '../theme/palette';
import { Icon, type IconName } from '../ui/icons';
import { useNow } from '../ui/shell/useNow';
import { ORIGINAL, PREVIEW_SIZE, useMedia } from '../ui/media';
import { AudioPreview } from '../ui/preview/AudioPreview';
import { GalleryStrip, STRIP_HEIGHT } from '../ui/preview/GalleryStrip';
import { useSwipe } from '../ui/preview/swipe';
import { saveAttachment, shareAttachment } from '../ui/timeline/files';

const MAX_ZOOM = 5;

const DOUBLE_TAP_ZOOM = 2.5;

/** The top bar's height under the status bar. */
const BAR = 56;

/** The bottom of a video the platform's controls take, which a swipe leaves to them. */
const VIDEO_CONTROLS = 120;

export default function PreviewScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
    thread?: string;
  }>();

  const now = useNow();
  const store = useAppStore();
  const insets = useSafeAreaInsets();

  const key = useMemo(
    () => (thread ? threadKey(roomId, thread) : roomKey(roomId)),
    [roomId, thread],
  );

  // The item on screen, which the strip and swipes move; the opened one at first.
  const [current, setCurrent] = useState(eventId);

  const gallery = useStore(
    (s) => galleryAround(timeline(s, account, key).entries, current, eventId, thread ?? null),
    shallow,
  );

  const found = gallery.findIndex((m) => m.event_id === current);
  const index = found < 0 ? gallery.findIndex((m) => m.event_id === eventId) : found;
  const message = gallery[index] ?? null;
  const attachment = message?.content.type === 'Attachment' ? message.content.data : null;
  const strip = showsStrip(gallery);
  const [busy, setBusy] = useState(false);

  const showActions = () => {
    if (message) router.push(routes.attachmentActions(account, roomId, message.event_id, thread));
  };

  const step = useCallback(
    (by: 1 | -1) => {
      const next = gallery[stepGallery(index, by, gallery.length)];

      if (next) setCurrent(next.event_id);
    },
    [gallery, index],
  );

  const onSwipe = strip ? step : null;
  const frame = { top: insets.top + BAR, bottom: insets.bottom + (strip ? STRIP_HEIGHT : 0) };

  const run = (action: typeof shareAttachment) => {
    if (!attachment) return;
    setBusy(true);
    void action(store, account, attachment).finally(() => setBusy(false));
  };

  return (
    <View style={styles.screen} testID="preview">
      <StatusBar style="light" />
      {message && attachment ? (
        // Keyed by message, so moving on releases the item before: a video or clip stops.
        attachment.kind === 'Video' ? (
          <VideoContent
            key={message.event_id}
            account={account}
            attachment={attachment}
            frame={frame}
            onSwipe={onSwipe}
          />
        ) : attachment.kind === 'Audio' ? (
          <AudioPreview
            key={message.event_id}
            account={account}
            attachment={attachment}
            style={frame}
            onSwipe={onSwipe}
            onLongPress={showActions}
          />
        ) : (
          <ImageContent
            key={message.event_id}
            account={account}
            attachment={attachment}
            frame={strip ? { top: 0, bottom: frame.bottom } : null}
            onSwipe={onSwipe}
            onLongPress={showActions}
          />
        )
      ) : (
        <Text style={[font.body, styles.status]}>{t('mobile.conversation.message_missing')}</Text>
      )}
      <View style={[styles.bar, { paddingTop: insets.top + 8 }]}>
        <BarButton
          icon="close"
          label={t('preview.close')}
          onPress={() => router.back()}
          testID="preview-close"
        />
        <View style={styles.who}>
          {message ? (
            <>
              <Text style={[font.bodyStrong, styles.white]} numberOfLines={1}>
                {message.sender_name}
              </Text>
              <Text style={[font.meta, styles.dim]} numberOfLines={1}>
                {sentAt(message.timestamp_ms, now)}
              </Text>
            </>
          ) : null}
        </View>
        {attachment?.media ? (
          <>
            {busy ? <ActivityIndicator color="#ffffff" style={styles.busy} /> : null}
            <BarButton
              icon="more"
              label={t('actions.more')}
              onPress={showActions}
              testID="preview-more"
            />
            <BarButton
              icon="share"
              label={t('mobile.conversation.share')}
              onPress={() => run(shareAttachment)}
              testID="preview-share"
            />
            {Platform.OS === 'android' ? (
              <BarButton
                icon="download"
                label={t('preview.download')}
                onPress={() => run(saveAttachment)}
                testID="preview-download"
              />
            ) : null}
          </>
        ) : null}
      </View>
      {strip && message ? (
        <GalleryStrip
          account={account}
          items={gallery}
          current={message.event_id}
          bottom={insets.bottom}
          onPick={setCurrent}
        />
      ) : null}
    </View>
  );
}

function BarButton({
  icon,
  label,
  onPress,
  testID,
}: {
  icon: IconName;
  label: string;
  onPress: () => void;
  testID: string;
}) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      hitSlop={8}
      testID={testID}
      style={({ pressed }) => [styles.button, { opacity: pressed ? 0.6 : 1 }]}
    >
      <Icon name={icon} size={20} color="#ffffff" />
    </Pressable>
  );
}

/** Where content goes on screen: clear of the top bar and the gallery's strip. */
interface Frame {
  top: number;
  bottom: number;
}

type Swipe = ((step: 1 | -1) => void) | null;

/** The image full size, its preview shown while the original loads. */
function ImageContent({
  account,
  attachment,
  frame,
  onSwipe,
  onLongPress,
}: {
  account: string;
  attachment: Attachment;
  /** Clear of the strip, when there is one; the whole screen otherwise. */
  frame: Frame | null;
  onSwipe: Swipe;
  onLongPress: () => void;
}) {
  const preview = attachmentPreview(attachment);
  const small = useMedia(account, preview?.media ?? null, preview?.size ?? PREVIEW_SIZE);
  const original = useMedia(account, attachment.media, ORIGINAL);
  const uri = original ?? small;
  const area = [StyleSheet.absoluteFill, frame];

  if (!uri) return <Loading style={area} onSwipe={onSwipe} />;

  return (
    <View style={area}>
      <Zoomable uri={uri} label={attachment.name} onSwipe={onSwipe} onLongPress={onLongPress} />
    </View>
  );
}

/** A file still coming: a spinner, swiped past like the file itself. */
function Loading({ style, onSwipe }: { style: StyleProp<ViewStyle>; onSwipe: Swipe }) {
  const swipe = useSwipe(onSwipe);

  return (
    <GestureDetector gesture={swipe}>
      <View style={[style, styles.loading]}>
        <ActivityIndicator color="#ffffff" />
      </View>
    </GestureDetector>
  );
}

/** Pinch to zoom, pan while zoomed, double-tap to zoom in or back out. */
function Zoomable({
  uri,
  label,
  onSwipe,
  onLongPress,
}: {
  uri: string;
  label: string;
  /** Steps through the gallery while the image isn't zoomed in. */
  onSwipe: Swipe;
  onLongPress: () => void;
}) {
  const scale = useSharedValue(1);
  const savedScale = useSharedValue(1);
  const x = useSharedValue(0);
  const y = useSharedValue(0);
  const savedX = useSharedValue(0);
  const savedY = useSharedValue(0);

  const pinch = usePinchGesture({
    onUpdate: (event) => {
      'worklet';
      scale.value = Math.min(MAX_ZOOM, Math.max(1, savedScale.value * event.scale));
    },
    onDeactivate: () => {
      'worklet';
      savedScale.value = scale.value;

      if (scale.value <= 1) {
        x.value = withTiming(0);
        y.value = withTiming(0);
        savedX.value = 0;
        savedY.value = 0;
      }
    },
  });

  const pan = usePanGesture({
    onUpdate: (event) => {
      'worklet';

      if (scale.value <= 1) return;
      x.value = savedX.value + event.translationX;
      y.value = savedY.value + event.translationY;
    },
    onDeactivate: () => {
      'worklet';
      savedX.value = x.value;
      savedY.value = y.value;
    },
  });

  const doubleTap = useTapGesture({
    numberOfTaps: 2,
    onActivate: () => {
      'worklet';
      const zoomed = scale.value > 1;
      const next = zoomed ? 1 : DOUBLE_TAP_ZOOM;
      scale.value = withTiming(next);
      savedScale.value = next;
      x.value = withTiming(0);
      y.value = withTiming(0);
      savedX.value = 0;
      savedY.value = 0;
    },
  });

  const longPress = useLongPressGesture({
    numberOfPointers: 1,
    minDuration: 350,
    runOnJS: true,
    onActivate: onLongPress,
  });

  // Zoomed in, a sideways drag pans the image instead of moving on.
  const swipe = useSwipe(onSwipe, { canSwipe: () => scale.value <= 1 && savedScale.value <= 1 });

  const gesture = useSimultaneousGestures(pinch, pan, doubleTap, longPress, swipe);

  const style = useAnimatedStyle(() => ({
    transform: [{ translateX: x.value }, { translateY: y.value }, { scale: scale.value }],
  }));

  return (
    <GestureDetector gesture={gesture}>
      <Animated.View style={[StyleSheet.absoluteFill, style]}>
        <Image
          source={{ uri }}
          style={StyleSheet.absoluteFill}
          contentFit="contain"
          accessibilityLabel={label}
          transition={120}
        />
      </Animated.View>
    </GestureDetector>
  );
}

/** A video in the platform's own player, once its file is here. */
function VideoContent({
  account,
  attachment,
  frame,
  onSwipe,
}: {
  account: string;
  attachment: Attachment;
  frame: Frame;
  onSwipe: Swipe;
}) {
  const uri = useMedia(account, attachment.media, ORIGINAL);

  if (!uri) return <Loading style={[StyleSheet.absoluteFill, frame]} onSwipe={onSwipe} />;

  return <VideoPlayer uri={uri} frame={frame} onSwipe={onSwipe} />;
}

/**
 * The player, made for this video and released when the preview moves on (the content is keyed
 * by message). A swipe over the picture steps through the gallery; the controls at the bottom
 * keep their own drags.
 */
function VideoPlayer({ uri, frame, onSwipe }: { uri: string; frame: Frame; onSwipe: Swipe }) {
  const player = useVideoPlayer(uri, (p) => {
    p.play();
  });

  const swipe = useSwipe(onSwipe, { bottomInset: VIDEO_CONTROLS });

  return (
    <GestureDetector gesture={swipe}>
      <View style={[StyleSheet.absoluteFill, frame]}>
        <VideoView
          player={player}
          style={StyleSheet.absoluteFill}
          contentFit="contain"
          nativeControls
        />
      </View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: '#000000' },
  loading: { alignItems: 'center', justifyContent: 'center' },
  status: { color: '#ffffffcc', textAlign: 'center', marginTop: 160 },
  bar: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: 12,
    paddingBottom: 10,
    backgroundColor: '#00000080',
  },
  who: { flex: 1, minWidth: 0 },
  white: { color: '#ffffff' },
  dim: { color: '#ffffffb3' },
  busy: { marginRight: 4 },
  button: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#ffffff1f',
  },
});
