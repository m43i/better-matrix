import type { CallInfo, CallParticipant, IncomingCall } from '@better-matrix/matrix-native/types';

import { callTiles, initialCallView } from '../../../state/callView';
import { roomSummary } from '../../../test/fixtures';
import {
  controls,
  describeTile,
  endedNotice,
  headerCall,
  hear,
  nextCamera,
  ringLabel,
  showsVideo,
  SPEAKING_HOLD_MS,
  SPEAKING_LEVEL,
  stageStatus,
  stoppedNotice,
} from '../stage';

const ME = '@ada:example.org';

function person(key: string, change: Partial<CallParticipant> = {}): CallParticipant {
  return { key, person: { id: `@${key}:x`, name: key, avatar: null }, muted: false, camera: null, screen: null, ...change };
}

function info(change: Partial<CallInfo> = {}): CallInfo {
  return {
    call: 1,
    account: ME,
    room_id: '!r',
    via: 'Rtc',
    phase: 'Connected',
    video: false,
    microphone: true,
    camera: null,
    screen: null,
    participants: [person('grace')],
    ...change,
  };
}

describe('stageStatus', () => {
  it('should name whom a one-to-one call is calling', () => {
    expect(stageStatus(info({ phase: 'Calling' }))).toBe('Calling grace…');
  });

  it('should say it is connecting while joining', () => {
    expect(stageStatus(info({ phase: 'Connecting' }))).toBe('Connecting…');
  });

  it('should say it is reconnecting after the connection dropped', () => {
    expect(stageStatus(info({ phase: 'Reconnecting' }))).toBe('Reconnecting…');
  });

  it('should say it waits for others while the user is alone in the call', () => {
    expect(stageStatus(info({ participants: [] }))).toBe('Waiting for others to join');
  });

  it('should say nothing while the call is simply under way', () => {
    expect(stageStatus(info())).toBeNull();
  });
});

describe('showsVideo', () => {
  it('should show faces for a voice call without cameras', () => {
    expect(showsVideo(info())).toBe(false);
  });

  it('should show the grid once someone turns a camera on', () => {
    expect(showsVideo(info({ participants: [person('grace', { camera: 3 })] }))).toBe(true);
  });
});

describe('tiles', () => {
  const tilesOf = (call: CallInfo) => callTiles(call, initialCallView()).map((tile) => describeTile(call, tile.key));

  it('should put everyone else first and the user last', () => {
    expect(tilesOf(info({ participants: [person('a'), person('b')] })).map((tile) => tile.name)).toEqual(['a', 'b', 'You']);
  });

  it('should mark the user muted when their microphone is off', () => {
    expect(tilesOf(info({ microphone: false })).at(-1)?.muted).toBe(true);
  });

  it('should put a shared screen before the cameras', () => {
    const tiles = tilesOf(info({ participants: [person('grace', { camera: 2, screen: 5 })] }));
    expect(tiles.map((tile) => [tile.key, tile.video, tile.screen])).toEqual([
      ['screen-grace', 5, true],
      ['tile-grace', 2, false],
      ['tile-you', null, false],
    ]);
  });

  it('should mirror only the users own camera', () => {
    expect(tilesOf(info({ camera: 7 })).filter((tile) => tile.own).map((tile) => tile.video)).toEqual([7]);
  });

  it('should name a shared screen after whose it is', () => {
    expect(tilesOf(info({ participants: [person('grace', { screen: 5 })] }))[0]?.name).toBe("grace's screen");
  });
});

describe('controls', () => {
  it('should offer to unmute a muted microphone', () => {
    expect(controls(info({ microphone: false }), true).microphone).toEqual({ shown: true, on: false, label: 'Unmute' });
  });

  it('should offer to turn a filming camera off', () => {
    expect(controls(info({ camera: 4 }), true).camera).toEqual({ shown: true, on: true, label: 'Turn camera off' });
  });

  it('should hide the camera of a one-to-one voice call, which carries none', () => {
    expect(controls(info({ via: 'Direct', video: false }), true).camera.shown).toBe(false);
  });

  it('should hide the camera where the phone cannot film', () => {
    expect(controls(info(), false).camera.shown).toBe(false);
  });
});

describe('headerCall', () => {
  it('should hide the buttons where the room allows no call', () => {
    expect(headerCall(roomSummary('!r'), null, ME)).toEqual({ kind: 'none' });
  });

  it('should offer voice and video where a call can start', () => {
    const room = { ...roomSummary('!r'), call: { via: 'Rtc' as const, participants: [] } };
    expect(headerCall(room, null, ME)).toEqual({ kind: 'start' });
  });

  it('should offer to join a call under way with how many are in it', () => {
    const room = { ...roomSummary('!r'), call: { via: 'Rtc' as const, participants: ['@a:x', '@b:x'] } };
    expect(headerCall(room, null, ME)).toEqual({ kind: 'join', count: 2 });
  });

  it('should lead back to the stage while the user is in the rooms call', () => {
    const room = { ...roomSummary('!r'), call: { via: 'Rtc' as const, participants: [ME] } };
    expect(headerCall(room, info(), ME)).toEqual({ kind: 'return' });
  });
});

describe('notices', () => {
  it('should say nothing when someone simply hung up', () => {
    expect(endedNotice({ type: 'HungUp' })).toBeNull();
  });

  it('should say the call was declined', () => {
    expect(endedNotice({ type: 'Declined' })).toBe('The call was declined.');
  });

  it('should carry the reason a call failed', () => {
    expect(endedNotice({ type: 'Failed', data: 'no service' })).toBe('The call failed: no service');
  });

  it('should tell a stopped share from a stopped camera', () => {
    expect(stoppedNotice({ type: 'Screen', data: false })).toBe('Screen sharing stopped.');
    expect(stoppedNotice({ type: 'Camera', data: false })).toBe('Your camera stopped, so the call no longer shows it.');
  });

  it('should label a ring that calls with video', () => {
    const ring: IncomingCall = { account: ME, room_id: '!r', caller: { id: '@g:x', name: 'Grace', avatar: null }, video: true, via: 'Rtc' };
    expect(ringLabel(ring)).toBe('Incoming video call');
  });
});

describe('hear', () => {
  it('should count someone loud enough as speaking', () => {
    expect(hear({}, { grace: SPEAKING_LEVEL }, 0).speaking).toEqual(['grace']);
  });

  it('should keep someone speaking through a pause between words', () => {
    const first = hear({}, { grace: 200 }, 0);
    expect(hear(first.loudAt, { grace: 0 }, SPEAKING_HOLD_MS - 100).speaking).toEqual(['grace']);
  });

  it('should stop counting someone as speaking once the hold has passed', () => {
    const first = hear({}, { grace: 200 }, 0);
    expect(hear(first.loudAt, { grace: 0 }, SPEAKING_HOLD_MS).speaking).toEqual([]);
  });

  it('should not count someone quiet as speaking', () => {
    expect(hear({}, { grace: SPEAKING_LEVEL - 1 }, 0).speaking).toEqual([]);
  });
});

describe('nextCamera', () => {
  it('should switch from the default camera to the second', () => {
    expect(nextCamera(['front', 'back'], null)).toBe('back');
  });

  it('should come back round to the first', () => {
    expect(nextCamera(['front', 'back'], 'back')).toBe('front');
  });

  it('should have nothing to switch to without cameras', () => {
    expect(nextCamera([], null)).toBeNull();
  });
});
