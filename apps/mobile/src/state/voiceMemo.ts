// Recording a voice memo, as an explicit state machine. Two ways in, as in Slack: hold the mic
// (release sends, unless it was too short to be meant), or tap it (the recording row then offers
// discard and send). The view runs the effects (open the microphone, stop and send or discard)
// and reports back what happened.

/** Holds shorter than this are taken as a slip and discarded. */
export const SHORTEST_HOLD_MS = 700;

export type VoiceMemo =
  | { phase: 'idle' }
  /** Waiting for permission and the microphone; `released` when a hold ended meanwhile. */
  | { phase: 'starting'; held: boolean; released: boolean }
  | { phase: 'recording'; held: boolean; startedAt: number }
  /** The recorder is closing; `send` says what happens to the file. */
  | { phase: 'stopping'; send: boolean };

export type VoiceEvent =
  | { type: 'press'; held: boolean }
  | { type: 'release'; now: number }
  | { type: 'started'; now: number }
  | { type: 'failed' }
  | { type: 'send' }
  | { type: 'discard' }
  | { type: 'stopped' };

/** What the view does after a step. */
export type VoiceEffect = 'start' | 'send' | 'discard' | null;

export const VOICE_IDLE: VoiceMemo = { phase: 'idle' };

export interface VoiceTransition {
  next: VoiceMemo;
  effect: VoiceEffect;
}

export function voiceStep(state: VoiceMemo, event: VoiceEvent): VoiceTransition {
  const stay = { next: state, effect: null };

  switch (state.phase) {
    case 'idle':
      return event.type === 'press'
        ? { next: { phase: 'starting', held: event.held, released: false }, effect: 'start' }
        : stay;
    case 'starting':
      switch (event.type) {
        case 'release':
          return state.held ? { next: { ...state, released: true }, effect: null } : stay;
        case 'started':
          // A hold that ended before the microphone opened wasn't meant as a memo.
          return state.released
            ? { next: { phase: 'stopping', send: false }, effect: 'discard' }
            : {
                next: { phase: 'recording', held: state.held, startedAt: event.now },
                effect: null,
              };
        case 'failed':
          return { next: VOICE_IDLE, effect: null };
        case 'discard':
          return { next: { ...state, released: true }, effect: null };
        default:
          return stay;
      }

    case 'recording':
      switch (event.type) {
        case 'release':
          if (!state.held) return stay;

          return event.now - state.startedAt < SHORTEST_HOLD_MS
            ? { next: { phase: 'stopping', send: false }, effect: 'discard' }
            : { next: { phase: 'stopping', send: true }, effect: 'send' };
        case 'send':
          return { next: { phase: 'stopping', send: true }, effect: 'send' };
        case 'discard':
          return { next: { phase: 'stopping', send: false }, effect: 'discard' };
        case 'failed':
          return { next: VOICE_IDLE, effect: null };
        default:
          return stay;
      }

    case 'stopping':
      return event.type === 'stopped' || event.type === 'failed'
        ? { next: VOICE_IDLE, effect: null }
        : stay;
  }
}

/**
 * How long a memo has recorded as its clock shows it: whole seconds, so the composer redraws
 * once a second rather than at every tick.
 */
export function clockElapsed(ms: number): number {
  return Math.max(0, Math.floor(ms / 1000) * 1000);
}
