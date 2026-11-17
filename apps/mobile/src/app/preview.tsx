// The attachment preview, full screen on black: an image to pinch, pan and double-tap to zoom,
// or a video in the platform's player; who sent it and when at the top, with share, save and
// close.

import type { Attachment } from '@better-matrix/matrix-native/types';
import { Image } from 'expo-image';
import { router, useLocalSearchParams } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useVideoPlayer, VideoView } from 'expo-video';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Platform, Pressable, StyleSheet, Text, View } from 'react-native';
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
import { attachmentPreview, findMessage } from '../state/messages';
import { useAppStore, useStore } from '../state/react';
import { timeline } from '../state/selectors';
import { roomKey, threadKey } from '../state/timeline';
import { sentAt } from '../state/timelineText';
import { font } from '../theme/palette';
import { Icon, type IconName } from '../ui/icons';
import { useNow } from '../ui/shell/useNow';
import { ORIGINAL, PREVIEW_SIZE, useMedia } from '../ui/media';
import { saveAttachment, shareAttachment } from '../ui/timeline/files';

const MAX_ZOOM = 5;

const DOUBLE_TAP_ZOOM = 2.5;

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

  const message = useStore((s) => findMessage(timeline(s, account, key).entries, eventId));
  const attachment = message?.content.type === 'Attachment' ? message.content.data : null;
  const [busy, setBusy] = useState(false);
  const showActions = () => router.push(routes.attachmentActions(account, roomId, eventId, thread));

  const run = (action: typeof shareAttachment) => {
    if (!attachment) return;
    setBusy(true);
    void action(store, account, attachment).finally(() => setBusy(false));
  };

  return (
    <View style={styles.screen} testID="preview">
      <StatusBar style="light" />
      {attachment ? (
        attachment.kind === 'Video' ? (
          <VideoContent account={account} attachment={attachment} />
        ) : (
          <ImageContent account={account} attachment={attachment} onLongPress={showActions} />
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

/** The image full size, its preview shown while the original loads. */
function ImageContent({
  account,
  attachment,
  onLongPress,
}: {
  account: string;
  attachment: Attachment;
  onLongPress: () => void;
}) {
  const preview = attachmentPreview(attachment);
  const small = useMedia(account, preview?.media ?? null, preview?.size ?? PREVIEW_SIZE);
  const original = useMedia(account, attachment.media, ORIGINAL);
  const uri = original ?? small;

  if (!uri) return <ActivityIndicator color="#ffffff" style={StyleSheet.absoluteFill} />;

  return <Zoomable uri={uri} label={attachment.name} onLongPress={onLongPress} />;
}

/** Pinch to zoom, pan while zoomed, double-tap to zoom in or back out. */
function Zoomable({
  uri,
  label,
  onLongPress,
}: {
  uri: string;
  label: string;
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
    minDuration: 350,
    runOnJS: true,
    onActivate: onLongPress,
  });

  const gesture = useSimultaneousGestures(pinch, pan, doubleTap, longPress);

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
function VideoContent({ account, attachment }: { account: string; attachment: Attachment }) {
  const uri = useMedia(account, attachment.media, ORIGINAL);

  if (!uri) return <ActivityIndicator color="#ffffff" style={StyleSheet.absoluteFill} />;

  return <VideoPlayer uri={uri} />;
}

function VideoPlayer({ uri }: { uri: string }) {
  const insets = useSafeAreaInsets();

  const player = useVideoPlayer(uri, (p) => {
    p.play();
  });

  return (
    <VideoView
      player={player}
      style={[StyleSheet.absoluteFill, { top: insets.top + 56, bottom: insets.bottom }]}
      contentFit="contain"
      nativeControls
    />
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: '#000000' },
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
