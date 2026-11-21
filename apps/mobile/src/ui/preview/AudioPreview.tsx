// An audio clip in the full-screen preview: a large tile with the clip's name, a large play
// button, a scrubber to drag or press along, and the time played beside the length. One clip
// plays at a time across the app (`audioFocus`), and the speed is the timeline cards' own.

import type { AccountId, Attachment } from '@better-matrix/matrix-native/types';
import { useAudioPlayer, useAudioPlayerStatus } from 'expo-audio';
import { useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  type GestureResponderEvent,
  Pressable,
  type StyleProp,
  StyleSheet,
  Text,
  View,
  type ViewStyle,
} from 'react-native';
import { GestureDetector } from 'react-native-gesture-handler';

import { t } from '../../i18n';
import { cyclePlaybackSpeed } from '../../state/actions/core';
import { fetchOriginal } from '../../state/actions/timeline';
import { playTime } from '../../state/messages';
import { useAppStore, useStore } from '../../state/react';
import { humanSize } from '../../state/timelineText';
import { voiceDetails } from '../../state/voiceCard';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { claimAudio, releaseAudio } from '../timeline/audioFocus';
import { useSwipe } from './swipe';

const TILE = 168;

const PLAY = 72;

const SIDE = 48;

const KNOB = 16;

const TRACK = 4;

/** The scrubber's touch area, taller than the track it draws. */
const SCRUB_HEIGHT = 32;

/** How far one step of a screen reader's adjust moves the clip, as a share of its length. */
const STEP = 0.05;

interface Props {
  account: AccountId;
  attachment: Attachment;
  /** Where on screen it goes: clear of the top bar and the gallery's strip. */
  style: StyleProp<ViewStyle>;
  /** Steps through the gallery; `null` when there is nothing else to step to. */
  onSwipe: ((step: 1 | -1) => void) | null;
  onLongPress: () => void;
}

/** The clip, downloaded once the preview shows it, then played in a player of its own. */
export function AudioPreview(props: Props) {
  const store = useAppStore();
  const media = props.attachment.media;
  /** The clip's file once here, or why it couldn't be fetched. */
  const [file, setFile] = useState<{ uri: string } | { error: string } | null>(null);
  /** Counts presses on Try again: each fetches the file again. */
  const [attempt, setAttempt] = useState(0);

  // The timeline card's fetch, which fails rather than waiting for a file that won't come.
  useEffect(() => {
    if (!media) return;
    let current = true;

    fetchOriginal(store, props.account, media).then(
      (uri) => current && setFile({ uri }),
      (cause: unknown) => current && setFile({ error: String(cause) }),
    );

    return () => {
      current = false;
    };
  }, [store, props.account, media, attempt]);

  if (!file || 'error' in file) {
    const seconds = voiceDetails(props.attachment.audio).seconds;

    return (
      <AudioFace
        {...props}
        progress={0}
        elapsed={null}
        total={seconds}
        playing={false}
        loading={file === null}
        error={file?.error ?? null}
        onRetry={() => {
          setFile(null);
          setAttempt((n) => n + 1);
        }}
        speed={null}
      />
    );
  }

  return <LoadedAudio {...props} uri={file.uri} />;
}

function LoadedAudio({ uri, ...props }: Props & { uri: string }) {
  const store = useAppStore();
  const speed = useStore((s) => s.settings.playbackSpeed);
  const player = useAudioPlayer({ uri }, { updateInterval: 100 });
  const status = useAudioPlayerStatus(player);
  /** Where the knob is while a finger holds it, as a share of the length. */
  const [scrub, setScrub] = useState<number | null>(null);
  /** While a finger holds the scrubber: whether the clip plays on when it is let go. */
  const drag = useRef<boolean | null>(null);
  /** Where a paused clip rests after a seek: players report their place only while playing. */
  const [resting, setResting] = useState<number | null>(null);

  const duration =
    status.duration > 0 ? status.duration : voiceDetails(props.attachment.audio).seconds;

  const played = duration ? Math.min(1, status.currentTime / duration) : 0;
  // A clip that played to its end shows rewound (the player may still report the end).
  const finished = !status.playing && played >= 1;
  const progress = scrub ?? (status.playing ? played : (resting ?? (finished ? 0 : played)));

  useEffect(() => {
    player.setPlaybackRate(speed, 'high');
  }, [player, speed]);

  useEffect(() => () => releaseAudio(player), [player]);

  // A clip that plays to its end stops and rewinds, as the timeline's cards do.
  useEffect(() => {
    if (!status.didJustFinish) return;
    player.pause();
    void player.seekTo(0);
  }, [status.didJustFinish, player]);

  const start = () => {
    setResting(null);
    claimAudio(player);
    player.play();
  };

  const seek = (share: number, resume: boolean) => {
    const length = player.duration > 0 ? player.duration : (duration ?? 0);
    void player.seekTo(share * length);

    if (resume && share < 1) start();
    else setResting(share);
  };

  const toggle = () => {
    if (status.playing) return player.pause();
    const atEnd = status.duration > 0 && status.currentTime >= status.duration;

    if (atEnd) void player.seekTo(0);
    start();
  };

  return (
    <AudioFace
      {...props}
      progress={progress}
      elapsed={duration ? duration * progress : null}
      total={duration}
      playing={status.playing}
      loading={false}
      speed={speed}
      onSpeed={() => cyclePlaybackSpeed(store)}
      onToggle={toggle}
      onGrant={(share) => {
        drag.current = status.playing;

        if (status.playing) player.pause();
        setScrub(share);
      }}
      onMove={setScrub}
      onRelease={(share) => {
        seek(share, drag.current ?? false);
        drag.current = null;
        setScrub(null);
      }}
      onTerminate={() => {
        if (drag.current) start();
        drag.current = null;
        setScrub(null);
      }}
      onStep={(step) => seek(Math.min(1, Math.max(0, progress + step)), status.playing)}
    />
  );
}

function AudioFace({
  attachment,
  style,
  onSwipe,
  onLongPress,
  progress,
  elapsed,
  total,
  playing,
  loading,
  error = null,
  onRetry,
  speed,
  onSpeed,
  onToggle,
  onGrant,
  onMove,
  onRelease,
  onTerminate,
  onStep,
}: Props & {
  /** How far it has played (or where the knob is held), as a share of the length. */
  progress: number;
  /** Seconds played; `null` before anything is known. */
  elapsed: number | null;
  /** The length in seconds, when known. */
  total: number | null;
  playing: boolean;
  /** The clip is downloading. */
  loading: boolean;
  /** Why its file couldn't be fetched, with a way to try again. */
  error?: string | null;
  onRetry?: () => void;
  /** The playback speed, once the clip has loaded (its button shows then). */
  speed: number | null;
  onSpeed?: () => void;
  onToggle?: () => void;
  onGrant?: (share: number) => void;
  onMove?: (share: number) => void;
  onRelease?: (share: number) => void;
  onTerminate?: () => void;
  onStep?: (step: number) => void;
}) {
  const palette = usePalette();
  const [width, setWidth] = useState(0);
  const swipe = useSwipe(onSwipe);
  const details = voiceDetails(attachment.audio);
  const title = details.voice ? t('audio.voice') : attachment.name;
  const ready = !loading && onGrant !== undefined;

  const shareOf = (event: GestureResponderEvent) =>
    width > 0 ? Math.min(1, Math.max(0, event.nativeEvent.locationX / width)) : 0;

  return (
    <View style={[styles.frame, style]} testID="preview-audio">
      <GestureDetector gesture={swipe}>
        <Pressable
          onLongPress={onLongPress}
          delayLongPress={350}
          accessibilityLabel={title}
          style={styles.art}
        >
          <View style={[styles.tile, { backgroundColor: `${palette.accent}33` }]}>
            <Icon name={details.voice ? 'mic' : 'audio'} size={72} color={palette.accent} />
          </View>
          <Text style={[font.header, styles.white, styles.center]} numberOfLines={2}>
            {title}
          </Text>
          {attachment.size !== null ? (
            <Text style={[font.meta, styles.dim]}>{humanSize(attachment.size)}</Text>
          ) : null}
          {error !== null ? (
            <>
              <Text style={[font.body, styles.dim, styles.center]} testID="preview-audio-error">
                {t('notice.audio', { error })}
              </Text>
              <Pressable
                onPress={onRetry}
                accessibilityRole="button"
                hitSlop={8}
                style={styles.speed}
                testID="preview-audio-retry"
              >
                <Text style={[font.section, styles.white]}>{t('common.try_again')}</Text>
              </Pressable>
            </>
          ) : null}
        </Pressable>
      </GestureDetector>
      <View style={styles.controls}>
        <View
          style={styles.scrubber}
          onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
          onStartShouldSetResponder={() => ready}
          onResponderTerminationRequest={() => false}
          onResponderGrant={(event) => onGrant?.(shareOf(event))}
          onResponderMove={(event) => onMove?.(shareOf(event))}
          onResponderRelease={(event) => onRelease?.(shareOf(event))}
          onResponderTerminate={onTerminate}
          accessible
          accessibilityRole="adjustable"
          accessibilityLabel={t('audio.seek')}
          accessibilityValue={{ min: 0, max: 100, now: Math.round(progress * 100) }}
          accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
          onAccessibilityAction={(event) =>
            onStep?.(event.nativeEvent.actionName === 'increment' ? STEP : -STEP)
          }
          testID="preview-audio-seek"
        >
          <View style={styles.track} pointerEvents="none">
            <View
              style={[styles.fill, { width: progress * width, backgroundColor: palette.accent }]}
            />
          </View>
          <View
            pointerEvents="none"
            style={[
              styles.knob,
              { left: progress * width - KNOB / 2, backgroundColor: palette.accent },
            ]}
          />
        </View>
        <View style={styles.times}>
          <Text style={[font.meta, styles.dim, styles.digits]}>
            {elapsed !== null ? playTime(elapsed) : playTime(0)}
          </Text>
          <Text style={[font.meta, styles.dim, styles.digits]}>
            {total !== null ? playTime(total) : ''}
          </Text>
        </View>
        <View style={styles.buttons}>
          <View style={styles.side} />
          <Pressable
            onPress={onToggle}
            disabled={!ready}
            accessibilityRole="button"
            accessibilityLabel={playing ? t('audio.pause') : t('audio.play')}
            hitSlop={8}
            style={({ pressed }) => [
              styles.play,
              { backgroundColor: palette.accent, opacity: pressed ? 0.8 : 1 },
            ]}
            testID="preview-audio-play"
          >
            {loading ? (
              <ActivityIndicator color={palette.onAccent} />
            ) : (
              <Icon name={playing ? 'pause' : 'play'} size={34} color={palette.onAccent} />
            )}
          </Pressable>
          <View style={styles.side}>
            {speed !== null ? (
              <Pressable
                onPress={onSpeed}
                accessibilityRole="button"
                accessibilityLabel={t('audio.speed_label', { rate: speed })}
                hitSlop={8}
                style={styles.speed}
                testID="preview-audio-speed"
              >
                <Text style={[font.section, styles.white]}>
                  {t('audio.speed', { rate: speed })}
                </Text>
              </Pressable>
            ) : null}
          </View>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  frame: { position: 'absolute', left: 0, right: 0, paddingHorizontal: 24, paddingBottom: 16 },
  art: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 10 },
  tile: {
    width: TILE,
    height: TILE,
    borderRadius: 28,
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 14,
  },
  white: { color: '#ffffff' },
  dim: { color: '#ffffffb3' },
  center: { textAlign: 'center' },
  digits: { fontVariant: ['tabular-nums'] },
  controls: { gap: 4 },
  scrubber: { height: SCRUB_HEIGHT, justifyContent: 'center' },
  track: {
    height: TRACK,
    borderRadius: TRACK / 2,
    backgroundColor: '#ffffff33',
    overflow: 'hidden',
  },
  fill: { height: TRACK },
  knob: {
    position: 'absolute',
    top: (SCRUB_HEIGHT - KNOB) / 2,
    width: KNOB,
    height: KNOB,
    borderRadius: KNOB / 2,
  },
  times: { flexDirection: 'row', justifyContent: 'space-between' },
  buttons: {
    marginTop: 12,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 28,
  },
  side: { width: SIDE, alignItems: 'center' },
  play: {
    width: PLAY,
    height: PLAY,
    borderRadius: PLAY / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  speed: {
    minWidth: SIDE,
    height: 32,
    paddingHorizontal: 8,
    borderRadius: 16,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#ffffff1f',
  },
});
