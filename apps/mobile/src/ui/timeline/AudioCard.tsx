// An audio clip or voice memo, as the desktop's voice card: play beside the clip's waveform as
// bars that fill as it plays, a dot to drag (or a press anywhere along the bars) to jump, and
// the time under them; once it has loaded, a button beside the time steps its speed. A voice memo
// shows no file name; another audio file keeps its name above the bars.

import type { Attachment, Message } from '@better-matrix/matrix-native/types';
import { useAudioPlayer, useAudioPlayerStatus } from 'expo-audio';
import { router } from 'expo-router';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  type GestureResponderEvent,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
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

/** What a card does once its clip has loaded: play, or move to `share` (playing on if `resume`). */
type FirstAction = { type: 'play' } | { type: 'seek'; share: number; resume: boolean };

/** The card's waveform: how many bars fit, and how wide they are together. */
interface Wave {
  bars: number;
  width: number;
}

function useWave(): Wave {
  const bars = waveBars(
    Math.min(useContentWidth(), CARD_MAX) - PAD_LEFT - PAD_RIGHT - 2 - PLAY - PLAY_GAP,
  );

  return { bars, width: waveWidth(bars) };
}

/**
 * The card until its clip is wanted: no player yet, since a native player per card would cost
 * every voice memo scrolled past. Play, a press or drag along the bars, or a screen reader's
 * adjust downloads the clip, then the loaded card takes over and does what was asked.
 */
export function AudioCard({ message, attachment }: { message: Message; attachment: Attachment }) {
  const store = useAppStore();
  const { account } = useTimelineContext();
  const [busy, setBusy] = useState(false);
  /** Where the dot is while a finger drags it, as a share of the length. */
  const [scrub, setScrub] = useState<number | null>(null);
  const [clip, setClip] = useState<{ uri: string; first: FirstAction } | null>(null);
  const wave = useWave();
  const media = attachment.media;
  const details = voiceDetails(attachment.audio);

  if (clip)
    return (
      <LoadedAudioCard
        message={message}
        attachment={attachment}
        uri={clip.uri}
        first={clip.first}
      />
    );

  /** Downloads the clip once, then hands over to the loaded card. */
  const load = (first: FirstAction) => {
    if (!media || busy) return;
    setBusy(true);
    fetchOriginal(store, account, media)
      .then((uri) => setClip({ uri, first }))
      .catch((cause: unknown) =>
        store.update((s) => addNotice(s, t('notice.audio', { error: String(cause) }))),
      )
      .finally(() => setBusy(false));
  };

  const progress = scrub ?? 0;
  const duration = details.seconds;

  return (
    <CardFace
      message={message}
      attachment={attachment}
      wave={wave}
      progress={progress}
      time={scrub !== null && duration ? duration * progress : duration}
      playing={false}
      busy={busy}
      speed={null}
      onToggle={() => load({ type: 'play' })}
      onGrant={setScrub}
      onMove={setScrub}
      // Let go before loading: the clip goes there and plays.
      onRelease={(share) => {
        load({ type: 'seek', share, resume: true });
        setScrub(null);
      }}
      onTerminate={() => setScrub(null)}
      onStep={(step) =>
        load({ type: 'seek', share: Math.min(1, Math.max(0, progress + step)), resume: false })
      }
    />
  );
}

/** Where a paused clip rests after a seek to `share`: nowhere set while it plays on from there. */
function restingAt(share: number, resume: boolean): number | null {
  return resume && share < 1 ? null : share;
}

/** The card with its clip loaded into a player of its own. */
function LoadedAudioCard({
  message,
  attachment,
  uri,
  first,
}: {
  message: Message;
  attachment: Attachment;
  uri: string;
  first: FirstAction;
}) {
  const store = useAppStore();
  const speed = useStore((s) => s.settings.playbackSpeed);
  const player = useAudioPlayer({ uri }, { updateInterval: 100 });
  const status = useAudioPlayerStatus(player);
  /** Where the dot is while a finger drags it, as a share of the length. */
  const [scrub, setScrub] = useState<number | null>(null);
  /** While a finger drags along the bars: whether the clip plays on when it is let go. */
  const drag = useRef<boolean | null>(null);
  /** The clip was moved to its very end on purpose: it waits there for play. */
  const [parked, setParked] = useState(first.type === 'seek' && first.share >= 1);

  /**
   * Where a paused clip rests after a seek made here, as a share of the length: the player
   * reports its place only while it plays.
   */
  const [resting, setResting] = useState(
    first.type === 'seek' ? restingAt(first.share, first.resume) : null,
  );

  const details = voiceDetails(attachment.audio);
  const wave = useWave();
  const duration = status.duration > 0 ? status.duration : details.seconds;
  const played = duration ? Math.min(1, status.currentTime / duration) : 0;
  // A clip that played to its end shows rewound (the player still reports the end), unless it
  // was moved there on purpose.
  const finished = !status.playing && played >= 1 && !parked;
  const progress = scrub ?? (status.playing ? played : (resting ?? (finished ? 0 : played)));

  // The speed holds for every clip, at the clip's own pitch (both platforms keep it by default).
  useEffect(() => {
    player.setPlaybackRate(speed, 'high');
  }, [player, speed]);

  useEffect(() => () => releaseAudio(player), [player]);

  // A clip that plays to its end stops and rewinds, as on the desktop. Pausing matters on
  // Android, whose player otherwise plays again at the next seek. One moved to its very end
  // stays there.
  useEffect(() => {
    if (!status.didJustFinish) return;
    player.pause();

    if (!parked) void player.seekTo(0);
  }, [status.didJustFinish, player, parked]);

  /** Plays, pausing any other clip. */
  const start = () => {
    claimAudio(player);
    player.play();
  };

  const play = () => {
    setResting(null);
    start();
  };

  /** Moves the player to `share` of the way, playing on if `resume` (and short of the end). */
  const move = (share: number, resume: boolean) => {
    const length = player.duration > 0 ? player.duration : (duration ?? 0);

    if (share >= 1) player.pause();
    void player.seekTo(share * length);

    if (resume && share < 1) start();
  };

  /** Moves the clip, and the dot with it. */
  const seek = (share: number, resume: boolean) => {
    setParked(share >= 1);
    setResting(restingAt(share, resume));
    move(share, resume);
  };

  // What was asked for before the clip loaded, once (after the speed is set, above); the dot
  // started where it asked.
  const begun = useRef(false);
  useEffect(() => {
    if (begun.current) return;
    begun.current = true;

    if (first.type === 'play') start();
    else move(first.share, first.resume);
  });

  const atEnd =
    status.didJustFinish || (status.duration > 0 && status.currentTime >= status.duration);

  const toggle = () => {
    if (status.playing) return player.pause();

    setParked(false);

    if (atEnd) void player.seekTo(0);
    play();
  };

  const moved = scrub !== null || status.playing || progress > 0;

  return (
    <CardFace
      message={message}
      attachment={attachment}
      wave={wave}
      progress={progress}
      time={moved && duration ? duration * progress : duration}
      playing={status.playing}
      busy={false}
      speed={speed}
      onSpeed={() => cyclePlaybackSpeed(store)}
      onToggle={toggle}
      // Presses and drags along the bars; only letting go moves the clip, so a cancelled drag
      // (the list taking the gesture) leaves it where it was.
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
        if (drag.current) play();
        drag.current = null;
        setScrub(null);
      }}
      onStep={(step) => seek(Math.min(1, Math.max(0, progress + step)), status.playing)}
    />
  );
}

/** What the card draws, loaded or not; the bars report where along them a finger is. */
function CardFace({
  message,
  attachment,
  wave,
  progress,
  time,
  playing,
  busy,
  speed,
  onSpeed,
  onToggle,
  onGrant,
  onMove,
  onRelease,
  onTerminate,
  onStep,
}: {
  message: Message;
  attachment: Attachment;
  wave: Wave;
  /** How far it has played (or where the dot is held), as a share of the length. */
  progress: number;
  /** The time under the bars, in seconds; `null` when the length isn't known. */
  time: number | null;
  playing: boolean;
  /** The clip is downloading. */
  busy: boolean;
  /** The playback speed, once the clip has loaded (its button shows then). */
  speed: number | null;
  onSpeed?: () => void;
  onToggle: () => void;
  onGrant: (share: number) => void;
  onMove: (share: number) => void;
  onRelease: (share: number) => void;
  onTerminate: () => void;
  /** A screen reader's adjust, by a share of the length. */
  onStep: (step: number) => void;
}) {
  const palette = usePalette();
  const { account, timelineKey, thread, openActions } = useTimelineContext();
  const media = attachment.media;
  const details = voiceDetails(attachment.audio);
  const { bars, width } = wave;
  const waveform = attachment.audio?.waveform;
  const heights = useMemo(() => barHeights(waveform ?? [], bars), [waveform, bars]);
  const indent = PLAY + PLAY_GAP;
  const shareOf = (event: GestureResponderEvent) => shareAt(event.nativeEvent.locationX, width);

  return (
    <Pressable
      // Beside play, the bars and the speed, a tap opens the clip in the preview.
      onPress={() =>
        router.push(
          routes.preview(account, timelineKey.room_id, message.event_id, thread ?? undefined),
        )
      }
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
          onPress={onToggle}
          onLongPress={() => openActions(message)}
          delayLongPress={350}
          disabled={busy || !media}
          accessibilityRole="button"
          accessibilityLabel={playing ? t('audio.pause') : t('audio.play')}
          hitSlop={6}
          style={[styles.play, { backgroundColor: palette.accent }]}
          testID={`audio-play-${message.event_id}`}
        >
          {busy ? (
            <ActivityIndicator color={palette.onAccent} />
          ) : (
            <Icon name={playing ? 'pause' : 'play'} size={18} color={palette.onAccent} />
          )}
        </Pressable>
        <View
          style={{ width, height: WAVE_HEIGHT }}
          onStartShouldSetResponder={() => Boolean(media)}
          onResponderTerminationRequest={() => false}
          onResponderGrant={(event) => onGrant(shareOf(event))}
          onResponderMove={(event) => onMove(shareOf(event))}
          onResponderRelease={(event) => onRelease(shareOf(event))}
          onResponderTerminate={onTerminate}
          accessible
          accessibilityRole="adjustable"
          accessibilityLabel={t('audio.seek')}
          accessibilityValue={{ min: 0, max: 100, now: Math.round(progress * 100) }}
          accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
          onAccessibilityAction={(event) =>
            onStep(event.nativeEvent.actionName === 'increment' ? STEP : -STEP)
          }
          testID={`audio-wave-${message.event_id}`}
        >
          <View style={styles.bars} pointerEvents="none">
            {heights.map((height, i) => (
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
        {speed !== null ? (
          <Pressable
            onPress={onSpeed}
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
