// How the user sees the call under way, as the desktop's `state::call` keeps it: the call layout
// and its page, self-view, Full video / Fill tile per camera, the participant strip's share,
// whether the call's room shows its chat, and who has speaker focus. Pure: the reducer keeps a
// `CallView` beside the engine's `CallInfo`, the stage follows the levels with `hearCall`, and
// both stages plan with `planCall` (ADR 0009, ADR 0010).

import type { CallInfo, CallParticipant } from '@better-matrix/matrix-native/types';

import {
  ASPECT,
  camera,
  type CallLayout,
  type CallPlan,
  type CallTile,
  plan,
  sameTile,
  screen,
  singlePlan,
  type TileGeometry,
  tileId,
} from './callLayout';

export const DEFAULT_STRIP = 0.24;

export const MIN_STRIP = 0.18;

export const MAX_STRIP = 0.4;

export interface CallView {
  layout: CallLayout;
  page: number;
  /** The user's own camera (or face) shows; hiding it doesn't stop the camera. */
  showSelf: boolean;
  /** Full video (`true`) or Fill tile (`false`) per camera, by `tileId`; unset follows the aspect. */
  fullVideo: Record<string, boolean>;
  /** The participant strip's share of the stage, 0.18 to 0.4. */
  strip: number;
  /** The call's room shows its conversation under a compact stage. */
  chat: boolean;
}

export function initialCallView(): CallView {
  return {
    layout: { type: 'Auto' },
    page: 0,
    showSelf: true,
    fullVideo: {},
    strip: DEFAULT_STRIP,
    chat: false,
  };
}

/** Each video's source width over height, by `VideoId`, where known. */
export type Aspects = Readonly<Record<number, number>>;

/** A video call shows video tiles: started with video, or anyone's camera or screen is on. */
export function callVideo(info: CallInfo): boolean {
  return (
    info.video ||
    info.camera !== null ||
    info.screen !== null ||
    info.participants.some((p) => p.camera !== null || p.screen !== null)
  );
}

export function participantOf(info: CallInfo, tile: CallTile): CallParticipant | undefined {
  return info.participants.find((p) => p.key === tile.participant);
}

/** The video a tile shows now, if any: a camera restarting keeps its tile with a new video. */
export function videoOf(info: CallInfo, tile: CallTile): number | null {
  if (tile.participant === '') return tile.screen ? info.screen : info.camera;
  const person = participantOf(info, tile);

  if (!person) return null;

  return tile.screen ? person.screen : person.camera;
}

export function aspectOf(info: CallInfo, tile: CallTile, aspects: Aspects): number {
  const video = videoOf(info, tile);

  return (video !== null ? aspects[video] : undefined) ?? ASPECT;
}

/** Every surface: screens (everyone's, then the user's), then cameras (the user's last). */
export function callTiles(info: CallInfo, view: CallView, aspects: Aspects = {}): TileGeometry[] {
  const screens = [
    ...info.participants.filter((p) => p.screen !== null).map((p) => screen(p.key)),
    ...(info.screen !== null ? [screen('')] : []),
  ];

  const cameras = [
    ...info.participants.map((p) => camera(p.key)),
    ...(view.showSelf ? [camera('')] : []),
  ];

  return [...screens, ...cameras].map((key) => ({ key, aspect: aspectOf(info, key, aspects) }));
}

/**
 * The call plan for a viewport of `size` points. `compact` is the stage above the chat: a video
 * call shows one surface at a time there, paging through every one of them.
 */
export function planCall(
  info: CallInfo,
  view: CallView,
  size: [number, number],
  speaker: string | null,
  compact: boolean,
  aspects: Aspects = {},
): CallPlan {
  const tiles = callTiles(info, view, aspects);
  const video = callVideo(info);
  const single = compact && video;

  const planned = plan(
    tiles,
    size,
    view.layout,
    speaker,
    single ? 0 : view.page,
    view.strip,
    !video,
  );

  const main = planned.tiles[0];

  if (!single || !main) return planned;
  const page = Math.min(view.page, Math.max(tiles.length - 1, 0));

  const selected =
    page === 0
      ? tiles.find((tile) => sameTile(tile.key, main.key))
      : tiles.filter((tile) => !sameTile(tile.key, main.key))[page - 1];

  if (!selected) return planned;

  return { ...singlePlan(selected.key, size, selected.aspect), page, pages: tiles.length };
}

/** The videos the plan shows: the only ones that get a native view, and so the only ones watched. */
export function plannedVideos(info: CallInfo, planned: CallPlan): Set<number> {
  const videos = new Set<number>();

  for (const tile of planned.tiles) {
    const video = videoOf(info, tile.key);

    if (video !== null) videos.add(video);
  }

  return videos;
}

/**
 * The view after the engine's latest `CallInfo`: preferences for surfaces that went are dropped,
 * and a pin on a participant who left (or a share that ended) returns to Auto.
 */
export function reconcileView(view: CallView, info: CallInfo): CallView {
  const tiles = callTiles(info, view);
  const present = (tile: CallTile) => tiles.some((t) => sameTile(t.key, tile));
  const ids = new Set(tiles.map((t) => tileId(t.key)));
  const kept = Object.entries(view.fullVideo).filter(([id]) => ids.has(id));

  const fullVideo =
    kept.length === Object.keys(view.fullVideo).length ? view.fullVideo : Object.fromEntries(kept);

  const pinGone =
    view.layout.type === 'Focus' && view.layout.pin !== null && !present(view.layout.pin);

  if (!pinGone && fullVideo === view.fullVideo) return view;

  return { ...view, fullVideo, layout: pinGone ? { type: 'Auto' } : view.layout };
}

/** Choosing a layout starts at its first page; choosing Auto (or Focus without a pin) releases a pin. */
export function withLayout(view: CallView, layout: CallLayout): CallView {
  return { ...view, layout, page: 0 };
}

/** Selecting a tile pins that surface in Focus. */
export function pinned(view: CallView, tile: CallTile): CallView {
  return withLayout(view, { type: 'Focus', pin: tile });
}

/** Hides or shows self-view; a pin on the user's own camera goes with it. */
export function toggledSelf(view: CallView): CallView {
  const showSelf = !view.showSelf;

  const pinnedSelf =
    view.layout.type === 'Focus' &&
    view.layout.pin !== null &&
    sameTile(view.layout.pin, camera(''));

  return {
    ...view,
    showSelf,
    page: 0,
    layout: !showSelf && pinnedSelf ? { type: 'Auto' } : view.layout,
  };
}

/** Screens always show their full image; portrait cameras default to it, others fill their tile. */
export function showsFullVideo(
  info: CallInfo,
  view: CallView,
  tile: CallTile,
  aspects: Aspects = {},
): boolean {
  if (tile.screen) return true;

  return view.fullVideo[tileId(tile)] ?? aspectOf(info, tile, aspects) < 1;
}

export function toggledFit(
  info: CallInfo,
  view: CallView,
  tile: CallTile,
  aspects: Aspects = {},
): CallView {
  return {
    ...view,
    fullVideo: { ...view.fullVideo, [tileId(tile)]: !showsFullVideo(info, view, tile, aspects) },
  };
}

/** The participant strip's share, kept within 18–40% of the stage. */
export function clampStrip(strip: number): number {
  return Math.min(Math.max(strip, MIN_STRIP), MAX_STRIP);
}

export function withStrip(view: CallView, strip: number): CallView {
  return { ...view, strip: clampStrip(strip) };
}

/** The page after (`forward`) or before the one the plan shows, within its pages. */
export function paged(view: CallView, planned: CallPlan, forward: boolean): CallView {
  const page = forward
    ? Math.min(planned.page + 1, Math.max(planned.pages - 1, 0))
    : Math.max(planned.page - 1, 0);

  return { ...view, page };
}

// ----- who is speaking, and who has speaker focus --------------------------------------------

/** Loudness (0 to 255) from which someone counts as speaking: the desktop's 0.06. */
export const SPEAKING_LEVEL = Math.ceil(0.06 * 255);

/** How long someone still counts as speaking after going quiet (ms), so rings don't flicker. */
export const SPEAKING_HOLD_MS = 400;

/** How long someone must stay the loudest speaker (ms) to take focus. */
export const SPEAKER_HOLD_MS = 1500;

/** When each participant was last loud enough, by participant key ("" for the user). */
export type LoudAt = Record<string, number>;

export interface HeardLevels {
  loudAt: LoudAt;
  speaking: string[];
}

export interface SpeakerUpdate {
  focus: SpeakerFocus;
  changed: boolean;
}

export interface AudioUpdate {
  audio: CallAudio;
  changed: boolean;
}

/** Takes everyone's loudness at `now`: who counts as speaking, and the times to keep. */
export function hear(loudAt: LoudAt, levels: Record<string, number>, now: number): HeardLevels {
  const next: LoudAt = {};

  for (const [key, at] of Object.entries(loudAt)) {
    if (now - at < SPEAKING_HOLD_MS) next[key] = at;
  }

  for (const [key, level] of Object.entries(levels)) {
    if (level >= SPEAKING_LEVEL) next[key] = now;
  }

  return { loudAt: next, speaking: Object.keys(next).sort() };
}

/** Switching focus needs sustained speech; the speaking rings stay responsive. */
export type SpeakerFocus =
  | { type: 'Idle' }
  | { type: 'Steady'; key: string }
  | { type: 'Switching'; current: string | null; candidate: string; since: number };

export function focusedSpeaker(focus: SpeakerFocus): string | null {
  switch (focus.type) {
    case 'Idle':
      return null;
    case 'Steady':
      return focus.key;
    case 'Switching':
      return focus.current;
  }
}

/** Hears the loudest candidate at `now`; `changed` when the focused speaker changed. */
export function heardSpeaker(
  focus: SpeakerFocus,
  candidate: string | null,
  now: number,
): SpeakerUpdate {
  const current = focusedSpeaker(focus);

  if (candidate === null || candidate === current) {
    return {
      focus: current === null ? { type: 'Idle' } : { type: 'Steady', key: current },
      changed: false,
    };
  }

  if (focus.type === 'Switching' && focus.candidate === candidate) {
    if (now - focus.since >= SPEAKER_HOLD_MS)
      return { focus: { type: 'Steady', key: candidate }, changed: true };

    return { focus, changed: false };
  }

  return { focus: { type: 'Switching', current, candidate, since: now }, changed: false };
}

/** What the stage follows of everyone's loudness. */
export interface CallAudio {
  loudAt: LoudAt;
  /** Who is speaking now, by participant key, sorted. */
  speaking: string[];
  focus: SpeakerFocus;
}

export function initialCallAudio(): CallAudio {
  return { loudAt: {}, speaking: [], focus: { type: 'Idle' } };
}

/**
 * Takes everyone's loudness (0 to 255) at `now`. The candidate for focus is the loudest unmuted
 * remote participant speaking; a speaker who left loses focus. `changed` when the speaking rings
 * or the focused speaker changed, which is all the stage shows of it.
 */
export function hearCall(
  audio: CallAudio,
  levels: Record<string, number>,
  participants: readonly CallParticipant[],
  now: number,
): AudioUpdate {
  const heard = hear(audio.loudAt, levels, now);
  const ringsChanged = heard.speaking.join('\n') !== audio.speaking.join('\n');
  const speaking = new Set(heard.speaking);
  let candidate: string | null = null;
  let loudest = -1;

  for (const p of participants) {
    if (p.muted || !speaking.has(p.key)) continue;
    const level = levels[p.key] ?? 0;

    if (level > loudest || (level === loudest && candidate !== null && p.key > candidate)) {
      candidate = p.key;
      loudest = level;
    }
  }

  const current = focusedSpeaker(audio.focus);
  const left = current !== null && !participants.some((p) => p.key === current);
  const focus = heardSpeaker(left ? { type: 'Idle' } : audio.focus, candidate, now);

  return {
    audio: { loudAt: heard.loudAt, speaking: heard.speaking, focus: focus.focus },
    changed: ringsChanged || focus.changed || left,
  };
}
