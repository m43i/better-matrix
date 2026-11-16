// What the call's views show, derived from the engine's `CallInfo`, as the desktop's `ui::call`
// derives it: the line above the people, what a planned tile shows (where tiles go is
// `state/callLayout`), the controls and their states, and how a ring and a call's end are worded.

import type {
  CallEnd,
  CallInfo,
  CallParticipant,
  IncomingCall,
  MediaToggle,
  RoomSummary,
} from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import type { CallTile } from '../../state/callLayout';
import { callVideo, participantOf, videoOf } from '../../state/callView';

/** What the stage says above the people, while the call isn't simply under way. */
export function stageStatus(info: CallInfo): string | null {
  switch (info.phase) {
    case 'Calling':
      return t('call.calling', { name: info.participants[0]?.person.name ?? '' });
    case 'Connecting':
      return t('call.connecting');
    case 'Reconnecting':
      return t('call.reconnecting');
    case 'Connected':
      return info.participants.length === 0 ? t('call.alone') : null;
  }
}

/** A video call shows video tiles: started with video, or anyone's camera or screen is on. */
export const showsVideo = callVideo;

/** One planned tile as the stage draws it: a video surface, or a face. */
export interface Tile {
  /** Stable for the call: the element key. */
  key: string;
  /** The participant key its levels come under ("" for the user). */
  speaker: string;
  name: string;
  /** The video it shows, when there is one. */
  video: number | null;
  /** A shared screen: shown whole, not cropped. */
  screen: boolean;
  /** Whose face it falls back to; `null` for the user. */
  person: CallParticipant['person'] | null;
  muted: boolean;
  /** The user's own camera: mirrored. */
  own: boolean;
}

/** What a planned call tile shows: whose it is, its video, its name and mute. */
export function describeTile(info: CallInfo, tile: CallTile): Tile {
  const person = participantOf(info, tile) ?? null;
  const name = person?.person.name ?? t('call.you');
  return {
    key: `${tile.screen ? 'screen' : 'tile'}-${tile.participant === '' ? 'you' : tile.participant}`,
    speaker: tile.screen ? '' : tile.participant,
    name: tile.screen ? t('call.screen_of', { name }) : name,
    video: videoOf(info, tile),
    screen: tile.screen,
    person: person?.person ?? null,
    muted: tile.screen ? false : (person?.muted ?? !info.microphone),
    own: !tile.screen && tile.participant === '',
  };
}

/** One of the stage's controls. */
export interface Control {
  /** Shown at all (one-to-one calls carry only the camera they started with). */
  shown: boolean;
  /** On: the microphone open, the camera filming, the loudspeaker playing. */
  on: boolean;
  label: string;
}

/** The microphone and camera controls, as the desktop shows them. */
export function controls(info: CallInfo, canFilm: boolean): { microphone: Control; camera: Control } {
  const filming = info.camera !== null;
  return {
    microphone: { shown: true, on: info.microphone, label: info.microphone ? t('call.mute') : t('call.unmute') },
    camera: {
      // Device to device, the connection carries the camera it started with (if any).
      shown: canFilm && (info.via === 'Rtc' || info.video),
      on: filming,
      label: filming ? t('call.camera_off') : t('call.camera_on'),
    },
  };
}

/** The call buttons a room's header shows. */
export type HeaderCall =
  | { kind: 'none' }
  /** Voice and video call. */
  | { kind: 'start' }
  /** A call is under way there: join it. */
  | { kind: 'join'; count: number }
  /** The user is in this room's call: back to its stage. */
  | { kind: 'return' };

export function headerCall(room: RoomSummary | undefined, current: CallInfo | null, account: string): HeaderCall {
  if (!room || room.call.via === null) return { kind: 'none' };
  if (current && current.account === account && current.room_id === room.room_id) return { kind: 'return' };
  if (room.call.participants.length > 0) return { kind: 'join', count: room.call.participants.length };
  return { kind: 'start' };
}

/** What a ring's card says under the caller's name. */
export function ringLabel(ring: IncomingCall): string {
  return ring.video ? t('call.incoming_video') : t('call.incoming');
}

/** The notice for a call's end, as the desktop words it; none when someone simply hung up. */
export function endedNotice(reason: CallEnd): string | null {
  switch (reason.type) {
    case 'HungUp':
    case 'Left':
      return null;
    case 'Declined':
      return t('call.declined');
    case 'NoAnswer':
      return t('call.no_answer');
    case 'AnsweredElsewhere':
      return t('call.answered_elsewhere');
    case 'Failed':
      return t('call.failed', { error: reason.data });
  }
}

/** The notice for a camera or screen share that stopped on its own. */
export function stoppedNotice(media: MediaToggle): string {
  return media.type === 'Screen' ? t('call.share_stopped') : t('call.camera_stopped');
}

// Who is speaking: the speaking hold lives with speaker focus (`state/callView`).
export { hear, type LoudAt, SPEAKING_HOLD_MS, SPEAKING_LEVEL } from '../../state/callView';

/** The camera after `current` among `names` (the first when `current` isn't one of them). */
export function nextCamera(names: string[], current: string | null): string | null {
  if (names.length === 0) return null;
  const at = current === null ? 0 : names.indexOf(current);
  return names[(at + 1) % names.length] ?? null;
}
