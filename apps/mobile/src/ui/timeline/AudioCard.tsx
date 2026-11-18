// An audio clip or voice memo, as the desktop's voice card: play beside the clip's waveform as
// bars that fill as it plays, a dot to drag (or a press anywhere along the bars) to jump, and
// the time under them; once it has loaded, a button beside the time steps its speed. A voice memo
// shows no file name; another audio file keeps its name above the bars.

import type { Attachment, Message } from '@better-matrix/matrix-native/types';
import { useAudioPlayer, useAudioPlayerStatus } from 'expo-audio';
import { useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  type GestureResponderEvent,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { cyclePlaybackSpeed } from '../../state/actions/core';
import { fetchOriginal } from '../../state/actions/timeline';
import { playTime } from '../../state/messages';
import { useAppStore, useStore } from '../../state/react';
import { addNotice } from '../../state/reducer';
import {
  WAVE_BAR,
  WAVE_GAP,
  WAVE_HEIGHT,
  barHeights,
  shareAt,
  voiceDetails,
  waveBars,
  waveWidth,
} from '../../state/voiceCard';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { claimAudio, releaseAudio } from './audioFocus';
import { useTimelineContext } from './context';
import { useContentWidth } from './readersLayout';

const PLAY = 36;

const PLAY_GAP = 10;

/** The card's padding, left and right, and its border. */
const PAD_LEFT = 10;

const PAD_RIGHT = 14;

/** The widest a card grows. */
const CARD_MAX = 420;

/** The scrubber's dot. */
const KNOB = 12;

/** How far one step of a screen reader's adjust moves the clip, as a share of its length. */
const STEP = 0.05;

export function AudioCard({ message, attachment }: { message: Message; attachment: Attachment }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, openActions } = useTimelineContext();
  const speed = useStore((s) => s.settings.playbackSpeed);
  const player = useAudioPlayer(null, { updateInterval: 100 });
  const status = useAudioPlayerStatus(player);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  /** Where the dot is while a finger drags it, as a share of the length. */
  const [scrub, setScrub] = useState<number | null>(null);
  /** While a finger drags along the bars: whether the clip plays on when it is let go. */
  const drag = useRef<boolean | null>(null);
  /** The clip was moved to its very end on purpose: it waits there for play. */
  const [parked, setParked] = useState(false);
  /**
   * Where a paused clip rests after a seek made here, as a share of the length: the player
   * reports its place only while it plays.
   */
  const [resting, setResting] = useState<number | null>(null);
  const media = attachment.media;
  const details = voiceDetails(attachment.audio);

  const bars = waveBars(
    Math.min(useContentWidth(), CARD_MAX) - PAD_LEFT - PAD_RIGHT - 2 - PLAY - PLAY_GAP,
  );

  const width = waveWidth(bars);
  const duration = status.duration > 0 ? status.duration : details.seconds;
  const played = loaded && duration ? Math.min(1, status.currentTime / duration) : 0;
  // A clip that played to its end shows rewound (the player still reports the end), unless it
  // was moved there on purpose.
  const finished = loaded && !status.playing && played >= 1 && !parked;
  const progress = scrub ?? (status.playing ? played : (resting ?? (finished ? 0 : played)));

  // The speed holds for every clip, at the clip's own pitch (both platforms keep it by default).
  useEffect(() => {
    if (!loaded) return;
    player.setPlaybackRate(speed, 'high');
  }, [player, speed, loaded]);

  useEffect(() => () => releaseAudio(player), [player]);

  // A clip that plays to its end stops and rewinds, as on the desktop. Pausing matters on
  // Android, whose player otherwise plays again at the next seek. One moved to its very end
  // stays there.
  useEffect(() => {
    if (!status.didJustFinish) return;
    player.pause();

    if (!parked) void player.seekTo(0);
  }, [status.didJustFinish, player, parked]);

  const play = () => {
    claimAudio(player);
    setResting(null);
    player.play();
  };

  /** Downloads the clip once, then runs `then`. */
  const load = (then: () => void) => {
    if (!media || busy) return;
    setBusy(true);
    fetchOriginal(store, account, media)
      .then((uri) => {
        player.replace({ uri });
        player.setPlaybackRate(speed, 'high');
        setLoaded(true);
        then();
      })
      .catch((cause: unknown) =>
        store.update((s) => addNotice(s, t('notice.audio', { error: String(cause) }))),
      )
      .finally(() => setBusy(false));
  };

  const atEnd =
    status.didJustFinish || (status.duration > 0 && status.currentTime >= status.duration);

  const toggle = () => {
    if (!loaded) return load(play);

    if (status.playing) return player.pause();

    setParked(false);

    if (atEnd) void player.seekTo(0);
    play();
  };

  /** Moves the clip to `share` of the way, playing on if `resume` (and short of the end). */
  const seek = (share: number, resume: boolean) => {
    const go = () => {
      const length = player.duration > 0 ? player.duration : (duration ?? 0);
      setParked(share >= 1);
      setResting(resume && share < 1 ? null : share);

      if (share >= 1) player.pause();
      void player.seekTo(share * length);

      if (resume && share < 1) play();
    };

    if (loaded) go();
    else load(go);
  };

  // Presses and drags along the bars; only letting go moves the clip, so a cancelled drag (the
  // list taking the gesture) leaves it where it was.
  const onGrant = (event: GestureResponderEvent) => {
    drag.current = status.playing || !loaded;

    if (status.playing) player.pause();
    setScrub(shareAt(event.nativeEvent.locationX, width));
  };

  const onMove = (event: GestureResponderEvent) =>
    setScrub(shareAt(event.nativeEvent.locationX, width));

  const onRelease = (event: GestureResponderEvent) => {
    const share = shareAt(event.nativeEvent.locationX, width);
    seek(share, drag.current ?? false);
    drag.current = null;
    setScrub(null);
  };

  const onTerminate = () => {
    if (drag.current && loaded) play();
    drag.current = null;
    setScrub(null);
  };

  const moved = scrub !== null || status.playing || progress > 0;
  const time = moved && duration ? duration * progress : duration;
  const indent = PLAY + PLAY_GAP;

  return (
    <Pressable
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      accessibilityLabel={details.voice ? t('audio.voice') : attachment.name}
      style={[styles.card, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      testID={`audio-${message.event_id}`}
    >
      {details.voice ? null : (
        <Text
          style={[font.bodyStrong, { color: palette.text, marginLeft: indent, width }]}
          numberOfLines={1}
        >
          {attachment.name}
        </Text>
      )}
      <View style={styles.row}>
        <Pressable
          onPress={toggle}
          onLongPress={() => openActions(message)}
          delayLongPress={350}
          disabled={busy || !media}
          accessibilityRole="button"
          accessibilityLabel={status.playing ? t('audio.pause') : t('audio.play')}
          hitSlop={6}
          style={[styles.play, { backgroundColor: palette.accent }]}
          testID={`audio-play-${message.event_id}`}
        >
          {busy ? (
            <ActivityIndicator color={palette.onAccent} />
          ) : (
            <Icon name={status.playing ? 'pause' : 'play'} size={18} color={palette.onAccent} />
          )}
        </Pressable>
        <View
          style={{ width, height: WAVE_HEIGHT }}
          onStartShouldSetResponder={() => Boolean(media)}
          onResponderTerminationRequest={() => false}
          onResponderGrant={onGrant}
          onResponderMove={onMove}
          onResponderRelease={onRelease}
          onResponderTerminate={onTerminate}
          accessible
          accessibilityRole="adjustable"
          accessibilityLabel={t('audio.seek')}
          accessibilityValue={{ min: 0, max: 100, now: Math.round(progress * 100) }}
          accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
          onAccessibilityAction={(event) => {
            const step = event.nativeEvent.actionName === 'increment' ? STEP : -STEP;
            seek(Math.min(1, Math.max(0, progress + step)), status.playing);
          }}
          testID={`audio-wave-${message.event_id}`}
        >
          <View style={styles.bars} pointerEvents="none">
            {barHeights(details.waveform, bars).map((height, i) => (
              <View
                key={i}
                style={[
                  styles.bar,
                  {
                    height,
                    backgroundColor:
                      (i + 0.5) / bars <= progress ? palette.accent : `${palette.textMuted}73`,
                  },
                ]}
              />
            ))}
          </View>
          <View
            pointerEvents="none"
            style={[
              styles.knob,
              {
                left: progress * width - KNOB / 2,
                backgroundColor: palette.accent,
                borderColor: palette.surface,
              },
            ]}
          />
        </View>
      </View>
      <View style={[styles.footer, { marginLeft: indent, width }]}>
        <Text style={[font.meta, { color: palette.textMuted, fontVariant: ['tabular-nums'] }]}>
          {time !== null ? playTime(time) : ''}
        </Text>
        {loaded ? (
          <Pressable
            onPress={() => cyclePlaybackSpeed(store)}
            accessibilityRole="button"
            accessibilityLabel={t('audio.speed_label', { rate: speed })}
            hitSlop={8}
            style={[styles.speed, { borderColor: palette.divider }]}
            testID={`audio-speed-${message.event_id}`}
          >
            <Text style={[font.meta, { color: palette.textMuted }]}>
              {t('audio.speed', { rate: speed })}
            </Text>
          </Pressable>
        ) : null}
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  card: {
    marginTop: 4,
    alignSelf: 'flex-start',
    paddingLeft: PAD_LEFT,
    paddingRight: PAD_RIGHT,
    paddingVertical: 8,
    gap: 2,
    borderRadius: 12,
    borderWidth: StyleSheet.hairlineWidth,
  },
  row: { flexDirection: 'row', alignItems: 'center', gap: PLAY_GAP },
  play: {
    width: PLAY,
    height: PLAY,
    borderRadius: PLAY / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  bars: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: WAVE_GAP },
  bar: { width: WAVE_BAR, borderRadius: WAVE_BAR / 2 },
  knob: {
    position: 'absolute',
    top: (WAVE_HEIGHT - KNOB) / 2,
    width: KNOB,
    height: KNOB,
    borderRadius: KNOB / 2,
    borderWidth: 2,
  },
  footer: {
    minHeight: 20,
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 4,
  },
  speed: {
    paddingHorizontal: 7,
    height: 20,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
