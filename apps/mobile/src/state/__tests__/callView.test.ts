// The desktop's `state::call` tests that apply to phones, one for one, and the watching tests of
// its `state::tests` as the plan decides them here (a planned video gets a native view, which
// watches it). Frame sizes and the glow are the desktop's own and have no counterpart.

import type { CallInfo, CallParticipant } from '@better-matrix/matrix-native/types';

import { camera, type CallTile, screen } from '../callLayout';
import {
  type CallAudio,
  type CallView,
  focusedSpeaker,
  hearCall,
  initialCallAudio,
  initialCallView,
  paged,
  planCall,
  plannedVideos,
  reconcileView,
  SPEAKER_HOLD_MS,
  SPEAKING_HOLD_MS,
  showsFullVideo,
  toggledFit,
  toggledSelf,
  videoOf,
  withLayout,
} from '../callView';

function info(participants: CallParticipant[], change: Partial<CallInfo> = {}): CallInfo {
  return {
    call: 1,
    account: '@me:example.org',
    room_id: '!r',
    via: 'Rtc',
    phase: 'Connected',
    video: true,
    microphone: true,
    camera: null,
    screen: null,
    participants,
    ...change,
  };
}

function person(
  key: string,
  cameraId: number | null,
  screenId: number | null = null,
): CallParticipant {
  return {
    key,
    person: { id: key, name: key, avatar: null },
    muted: false,
    camera: cameraId,
    screen: screenId,
  };
}

function focus(): CallView {
  return withLayout(initialCallView(), { type: 'Focus', pin: null });
}

/** Hears `levels` in order, one at each `[ms, levels]`. */
function listen(call: CallInfo, heard: Array<[number, Record<string, number>]>): CallAudio {
  let audio = initialCallAudio();

  for (const [at, levels] of heard) audio = hearCall(audio, levels, call.participants, at).audio;

  return audio;
}

const SIZE: [number, number] = [800, 450];

describe('speaking', () => {
  it('someone starting to speak should change the view', () => {
    const call = info([person('a', null)]);
    const first = hearCall(initialCallAudio(), { a: 200 }, call.participants, 0);
    const second = hearCall(first.audio, { a: 180 }, call.participants, 0);
    const third = hearCall(second.audio, { a: 0 }, call.participants, SPEAKING_HOLD_MS);
    expect([first.changed, second.changed, third.changed]).toEqual([true, false, true]);
  });

  it('a pause between words should not stop the speaking ring', () => {
    const call = info([person('a', null)]);
    const first = hearCall(initialCallAudio(), { a: 200 }, call.participants, 0);
    const pause = hearCall(first.audio, { a: 0 }, call.participants, 100);
    expect([pause.changed, pause.audio.speaking]).toEqual([false, ['a']]);
  });
});

describe('pins', () => {
  it('a pin should survive its camera restarting', () => {
    const pin = camera('a');

    const view = reconcileView(
      { ...initialCallView(), layout: { type: 'Focus', pin } },
      info([person('a', 2)]),
    );

    expect([view.layout, videoOf(info([person('a', 2)]), pin)]).toEqual([
      { type: 'Focus', pin },
      2,
    ]);
  });

  it('a departed pin should restore the automatic layout', () => {
    const view = reconcileView(
      { ...initialCallView(), layout: { type: 'Focus', pin: camera('a') } },
      info([]),
    );

    expect(view.layout).toEqual({ type: 'Auto' });
  });

  it('an ended pinned share should restore the automatic layout', () => {
    const view = reconcileView(
      { ...initialCallView(), layout: { type: 'Focus', pin: screen('a') } },
      info([person('a', 1)]),
    );

    expect(view.layout).toEqual({ type: 'Auto' });
  });

  it('an unchanged call should keep the same view', () => {
    const view = {
      ...initialCallView(),
      layout: { type: 'Focus', pin: camera('a') },
    } satisfies CallView;

    expect(reconcileView(view, info([person('a', 1)]))).toBe(view);
  });
});

describe('speaker focus', () => {
  const call = info([person('a', 1), person('b', 2)]);

  const main = (audio: CallAudio) =>
    planCall(call, focus(), SIZE, focusedSpeaker(audio.focus), false).tiles[0]?.key;

  it('a brief interjection should not switch focus', () => {
    const audio = listen(call, [
      [0, { a: 200 }],
      [SPEAKER_HOLD_MS, { a: 200 }],
      [SPEAKER_HOLD_MS + 100, { b: 240 }],
    ]);

    expect(main(audio)).toEqual(camera('a'));
  });

  it('sustained speech should switch focus', () => {
    const audio = listen(call, [
      [0, { b: 240 }],
      [SPEAKER_HOLD_MS, { b: 240 }],
    ]);

    expect(main(audio)).toEqual(camera('b'));
  });

  it('pauses between words should not restart speaker focus', () => {
    const ticks = Array.from({ length: 16 }, (_, tick): [number, Record<string, number>] => [
      tick * 100,
      { b: tick === 8 || tick === 12 ? 0 : 240 },
    ]);

    expect(main(listen(call, ticks))).toEqual(camera('b'));
  });

  it('a muted participant should not take focus', () => {
    const muted = info([person('a', 1), { ...person('b', 2), muted: true }]);

    const audio = listen(muted, [
      [0, { b: 240 }],
      [SPEAKER_HOLD_MS, { b: 240 }],
    ]);

    expect(focusedSpeaker(audio.focus)).toBeNull();
  });

  it('a speaker who left should lose focus', () => {
    const audio = listen(call, [
      [0, { b: 240 }],
      [SPEAKER_HOLD_MS, { b: 240 }],
    ]);

    const after = hearCall(audio, {}, [person('a', 1)], SPEAKER_HOLD_MS + 100);
    expect([after.changed, focusedSpeaker(after.audio.focus)]).toEqual([true, null]);
  });
});

describe('self-view and fit', () => {
  it('hiding self-view should keep the camera on', () => {
    const call = info([person('a', 1)], { camera: 2 });
    const view = toggledSelf(initialCallView());
    expect(plannedVideos(call, planCall(call, view, SIZE, null, false)).has(2)).toBe(false);
    expect(call.camera).toBe(2);
  });

  it('hiding self-view should release a pin on it', () => {
    const view = toggledSelf({ ...initialCallView(), layout: { type: 'Focus', pin: camera('') } });
    expect(view.layout).toEqual({ type: 'Auto' });
  });

  it('a portrait camera should offer both full and cropped views', () => {
    const call = info([person('a', 1)]);
    const key = camera('a');
    const view = toggledFit(call, initialCallView(), key, { 1: 0.5 });
    expect(view.fullVideo['camera:a']).toBe(false);
  });

  it('a landscape camera should fill its tile until asked for the full video', () => {
    const call = info([person('a', 1)]);
    const view = toggledFit(call, initialCallView(), camera('a'));
    expect([
      showsFullVideo(call, initialCallView(), camera('a')),
      showsFullVideo(call, view, camera('a')),
    ]).toEqual([false, true]);
  });

  it('a shared screen should always show its full image', () => {
    const call = info([person('a', 1, 3)]);
    expect(
      showsFullVideo(call, { ...initialCallView(), fullVideo: { 'screen:a': false } }, screen('a')),
    ).toBe(true);
  });
});

describe('paging and watching', () => {
  it('compact chat should page through every surface', () => {
    const call = info([person('a', 1, 3), person('b', 2)]);
    let view: CallView = { ...withLayout(initialCallView(), { type: 'Grid' }), chat: true };
    const seen: CallTile[] = [];

    for (let i = 0; i < 4; i++) {
      const planned = planCall(call, view, [360, 200], null, true);
      expect(planned.tiles).toHaveLength(1);
      seen.push(planned.tiles[0]!.key);
      view = paged(view, planned, true);
    }

    expect(new Set(seen.map((t) => `${t.screen}:${t.participant}`))).toEqual(
      new Set(
        [camera('a'), camera('b'), screen('a'), camera('')].map(
          (t) => `${t.screen}:${t.participant}`,
        ),
      ),
    );
  });

  it('paging a call should stop hidden videos and start visible ones', () => {
    const call = info(Array.from({ length: 10 }, (_, i) => person(String(i + 1), i + 1)));
    const view = withLayout(initialCallView(), { type: 'Grid' });
    const first = planCall(call, view, [300, 180], null, false);
    const second = planCall(call, paged(view, first, true), [300, 180], null, false);
    const before = plannedVideos(call, first);
    const after = plannedVideos(call, second);
    expect(before.size).toBeGreaterThan(0);
    expect([...before].some((video) => after.has(video))).toBe(false);
  });

  it('compact chat paging should watch only the selected video', () => {
    const call = info([person('1', 1), person('2', 2)]);
    const view = withLayout(initialCallView(), { type: 'Grid' });
    const first = planCall(call, view, [360, 200], null, true);
    const next = planCall(call, paged(view, first, true), [360, 200], null, true);
    expect([...plannedVideos(call, first)]).toEqual([1]);
    expect([...plannedVideos(call, next)]).toEqual([2]);
  });

  it('a compact voice call should page through faces rather than one at a time', () => {
    const call = info([person('a', null), person('b', null)], { video: false });
    expect(planCall(call, initialCallView(), [360, 110], null, true).tiles.length).toBeGreaterThan(
      1,
    );
  });

  it('paging past the last page should stay on it', () => {
    const call = info([person('1', 1), person('2', 2)]);
    const view = withLayout(initialCallView(), { type: 'Grid' });
    const planned = planCall(call, view, SIZE, null, false);
    expect(paged(view, planned, true).page).toBe(0);
  });
});
