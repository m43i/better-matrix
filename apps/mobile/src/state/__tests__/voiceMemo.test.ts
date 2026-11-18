import {
  clockElapsed,
  SHORTEST_HOLD_MS,
  type VoiceEvent,
  type VoiceMemo,
  VOICE_IDLE,
  voiceStep,
} from '../voiceMemo';

function run(events: VoiceEvent[], from: VoiceMemo = VOICE_IDLE) {
  let state = from;
  const effects = [];

  for (const event of events) {
    const step = voiceStep(state, event);
    state = step.next;

    if (step.effect) effects.push(step.effect);
  }

  return { state, effects };
}

describe('voiceStep', () => {
  it('should open the microphone on a press', () => {
    expect(run([{ type: 'press', held: false }]).effects).toEqual(['start']);
  });

  it('should send a held memo when the hold ends', () => {
    const { effects } = run([
      { type: 'press', held: true },
      { type: 'started', now: 0 },
      { type: 'release', now: 5000 },
    ]);

    expect(effects).toEqual(['start', 'send']);
  });

  it('should discard a hold too short to be meant', () => {
    const { effects } = run([
      { type: 'press', held: true },
      { type: 'started', now: 0 },
      { type: 'release', now: SHORTEST_HOLD_MS - 1 },
    ]);

    expect(effects).toEqual(['start', 'discard']);
  });

  it('should discard a hold that ended before the microphone opened', () => {
    const { effects } = run([
      { type: 'press', held: true },
      { type: 'release', now: 10 },
      { type: 'started', now: 20 },
    ]);

    expect(effects).toEqual(['start', 'discard']);
  });

  it('should keep a tapped memo recording when the finger lifts', () => {
    const { state } = run([
      { type: 'press', held: false },
      { type: 'started', now: 0 },
      { type: 'release', now: 5000 },
    ]);

    expect(state.phase).toBe('recording');
  });

  it('should send a tapped memo on send', () => {
    const { effects } = run([
      { type: 'press', held: false },
      { type: 'started', now: 0 },
      { type: 'send' },
    ]);

    expect(effects).toEqual(['start', 'send']);
  });

  it('should discard a memo on discard', () => {
    const { effects } = run([
      { type: 'press', held: false },
      { type: 'started', now: 0 },
      { type: 'discard' },
    ]);

    expect(effects).toEqual(['start', 'discard']);
  });

  it('should go back to idle when the microphone could not open', () => {
    expect(run([{ type: 'press', held: false }, { type: 'failed' }]).state).toEqual(VOICE_IDLE);
  });

  it('should go back to idle once the recorder stopped', () => {
    expect(run([{ type: 'stopped' }], { phase: 'stopping', send: true }).state).toEqual(VOICE_IDLE);
  });
});

describe('clockElapsed', () => {
  it('a recording should show the same time within one second', () => {
    expect(clockElapsed(1_250)).toBe(clockElapsed(1_999));
  });

  it('a recording should show the next second once it has passed', () => {
    expect(clockElapsed(2_000)).toBe(2_000);
  });
});
