// The voice card's arithmetic, as the desktop's (apps/desktop/src/ui/timeline.rs, `audio_card`):
// how many waveform bars fit, how tall each is, where a press on them lands, the playback speeds,
// and the waveform a recording sends from its microphone levels.

import type { AudioDetails } from '@better-matrix/matrix-native/types';

/** The waveform's bars: as many as fit the card, within these. */
export const WAVE_MAX_BARS = 36;

export const WAVE_MIN_BARS = 8;

export const WAVE_BAR = 3;

export const WAVE_GAP = 2;

export const WAVE_HEIGHT = 28;

/** The shortest bar, for silence or a clip whose waveform is unknown. */
export const WAVE_MIN_BAR = 3;

/** The loudest a waveform amplitude can be (MSC3246's unit, as `AudioDetails.waveform`). */
export const WAVEFORM_MAX = 1024;

/** Amplitudes a recorded memo sends: within the 30 to 120 MSC3246 suggests. */
export const WAVEFORM_POINTS = 100;

/** A waveform's quietest bar, in dBFS: room noise and below read as nothing. */
const WAVEFORM_FLOOR_DB = -50;

/** How many bars fit `width` points. */
export function waveBars(width: number): number {
  const fit = Math.floor((width + WAVE_GAP) / (WAVE_BAR + WAVE_GAP));

  return Math.min(WAVE_MAX_BARS, Math.max(WAVE_MIN_BARS, fit));
}

/** The width of `bars` bars and the gaps between them. */
export function waveWidth(bars: number): number {
  return bars * (WAVE_BAR + WAVE_GAP) - WAVE_GAP;
}

/**
 * `amplitudes` as `bars` heights from `WAVE_MIN_BAR` to `WAVE_HEIGHT`: each bar the loudest of its
 * stretch, scaled to the loudest bar, so a quiet memo still shows its shape. Flat when there are
 * none.
 */
export function barHeights(amplitudes: readonly number[], bars: number): number[] {
  if (amplitudes.length === 0 || bars <= 0)
    return Array.from({ length: Math.max(0, bars) }, () => WAVE_MIN_BAR);

  const peaks = Array.from({ length: bars }, (_, i) => {
    const start = Math.floor((i * amplitudes.length) / bars);

    const end = Math.min(
      amplitudes.length,
      Math.max(start + 1, Math.floor(((i + 1) * amplitudes.length) / bars)),
    );

    return Math.max(0, ...amplitudes.slice(Math.min(start, amplitudes.length - 1), end));
  });

  const loudest = Math.max(1, ...peaks);

  return peaks.map((peak) => WAVE_MIN_BAR + ((WAVE_HEIGHT - WAVE_MIN_BAR) * peak) / loudest);
}

/** Where a press `x` points along a waveform `width` wide lands, from 0 to 1. */
export function shareAt(x: number, width: number): number {
  return width > 0 ? Math.min(1, Math.max(0, x / width)) : 0;
}

/** How fast clips play: the speed button steps through these. */
export type PlaybackSpeed = 1 | 1.5 | 2;

export const SPEEDS: readonly PlaybackSpeed[] = [1, 1.5, 2];

/** The speed the button moves to: 1×, 1.5×, 2×, then 1× again. */
export function nextSpeed(speed: PlaybackSpeed): PlaybackSpeed {
  return SPEEDS[(SPEEDS.indexOf(speed) + 1) % SPEEDS.length] ?? 1;
}

/** A recorder's metering (dBFS) as a level from 0 (-60 dB and below) to 1 (full scale). */
export function meterLevel(db: number | undefined): number {
  return db === undefined ? 0 : Math.min(1, Math.max(0, (db + 60) / 60));
}

/**
 * The waveform a memo sends from its microphone levels (dBFS, in order): `points` amplitudes up
 * to `WAVEFORM_MAX`, each the loudest level of its stretch on a decibel scale from -50 dBFS, as
 * the desktop's. Empty for no levels.
 */
export function waveformFromLevels(levels: readonly number[], points = WAVEFORM_POINTS): number[] {
  if (levels.length === 0) return [];

  return Array.from({ length: points }, (_, i) => {
    const start = Math.floor((i * levels.length) / points);
    const end = Math.max(start + 1, Math.floor(((i + 1) * levels.length) / points));
    const db = Math.max(...levels.slice(start, Math.min(end, levels.length)));
    const level = Math.min(1, Math.max(0, (db - WAVEFORM_FLOOR_DB) / -WAVEFORM_FLOOR_DB));

    return Math.round(level * WAVEFORM_MAX);
  });
}

/** What a card shows of a clip's own description. */
export interface VoiceDetails {
  /** Whether it was sent as a voice memo: then it shows no file name. */
  voice: boolean;
  /** Its length, when the sender said. */
  seconds: number | null;
  waveform: readonly number[];
}

/** A clip's own description, as its card shows it. */
export function voiceDetails(audio: AudioDetails | null | undefined): VoiceDetails {
  const ms = audio?.duration_ms ?? null;

  return {
    voice: audio?.voice ?? false,
    seconds: ms === null ? null : ms / 1000,
    waveform: audio?.waveform ?? [],
  };
}
