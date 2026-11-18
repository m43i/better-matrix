import {
  detachedEngine,
  type DetachedEngine,
  type Engine,
  type EngineEvent,
} from '@better-matrix/matrix-native/client';
import type { MediaSize } from '@better-matrix/matrix-native/types';

import { wantLinkPreview, wantMedia } from '../actions/core';
import { FIRST_WAIT_MS, LONGEST_WAIT_MS } from '../retry';
import { Store } from '../store';
import { ADA, message, summary } from '../../test/fixtures';

function setup() {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  return { ...detached, store };
}

const latest = (room_id: string, id: string): EngineEvent => ({
  type: 'LatestMessage',
  data: { account: ADA, room_id, message: message(id) },
});

const typing = (room_id: string): EngineEvent => ({
  type: 'Typing',
  data: { account: ADA, room_id, people: [] },
});

const IMAGE = 'mxc://example.org/a';

const ORIGINAL: MediaSize = { type: 'Original' };

const fetches = (sent: DetachedEngine['sent']) =>
  sent.filter((s) => s.command.type === 'FetchMedia').length;

describe('Store', () => {
  it('a batch of events should tell the views once', async () => {
    const { store, engine } = setup();
    await Promise.resolve();
    let told = 0;
    store.subscribe(() => told++);
    engine.emitAll([latest('!a:x', '$1'), typing('!a:x'), latest('!b:x', '$2')]);
    await Promise.resolve();
    expect(told).toBe(1);
  });

  it('a batch should be taken in as its events one by one', () => {
    const batched = setup();
    const single = setup();

    const events = [
      latest('!a:x', '$1'),
      latest('!b:x', '$2'),
      typing('!a:x'),
      latest('!a:x', '$3'),
    ];

    batched.engine.emitAll(events);

    for (const event of events) single.engine.emit(event);
    const a = batched.store.getState().accounts[0]!;
    const b = single.store.getState().accounts[0]!;
    expect(a.latest).toEqual(b.latest);
    expect(a.typing).toEqual(b.typing);
    expect(Object.keys(a.heardAt)).toEqual(Object.keys(b.heardAt));
  });

  it('effects should hear every event of a batch in order', () => {
    const { store, engine } = setup();
    const heard: string[] = [];
    store.addEffect((event) => heard.push(event.type));
    engine.emitAll([latest('!a:x', '$1'), typing('!a:x'), latest('!b:x', '$2')]);
    expect(heard).toEqual(['LatestMessage', 'Typing', 'LatestMessage']);
  });

  it('an effect that throws should not lose the rest of the batch', () => {
    const { store, engine } = setup();

    store.addEffect((event) => {
      if (event.type === 'Typing') throw new Error('boom');
    });

    expect(() => engine.emitAll([typing('!a:x'), latest('!b:x', '$2')])).toThrow('boom');
    expect(store.getState().accounts[0]!.latest['!b:x']?.event_id).toBe('$2');
  });

  it('media fetched well should not be fetched again', () => {
    const { store, engine, sent } = setup();
    const release = wantMedia(store, ADA, IMAGE, ORIGINAL);
    const request = sent.at(-1)!.request;
    engine.emitAll([
      {
        type: 'MediaFile',
        data: { request, media: IMAGE, size: ORIGINAL, path: '/cache/a.png' },
      },
      { type: 'Finished', data: { request, result: { Ok: null } } },
    ]);
    release();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    expect(fetches(sent)).toBe(1);
  });

  it('several views wanting the same media should fetch it once', () => {
    const { store, sent } = setup();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    expect(fetches(sent)).toBe(1);
  });
});

describe('Store retrying failed fetches', () => {
  beforeEach(() => jest.useFakeTimers({ now: 1_000_000 }));
  afterEach(() => jest.useRealTimers());

  const fail = (engine: Engine, request: number) =>
    engine.emit({ type: 'Finished', data: { request, result: { Err: 'offline' } } });

  it('a view still waiting should get media again once a failed fetch waited', () => {
    const { store, engine, sent } = setup();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    fail(engine, sent.at(-1)!.request);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    expect(fetches(sent)).toBe(2);
  });

  it('media whose file could not be written should be fetched again once it waited', () => {
    const { store, engine, sent } = setup();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    const request = sent.at(-1)!.request;
    engine.emitAll([
      { type: 'MediaFailed', data: { request, error: 'disk full' } },
      { type: 'Finished', data: { request, result: { Ok: null } } },
    ]);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    expect(fetches(sent)).toBe(2);
  });

  it('media that failed should not be fetched while it waits', () => {
    const { store, engine, sent } = setup();
    const release = wantMedia(store, ADA, IMAGE, ORIGINAL);
    fail(engine, sent.at(-1)!.request);
    release();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    jest.advanceTimersByTime(FIRST_WAIT_MS - 1);
    expect(fetches(sent)).toBe(1);
  });

  it('media no view wants any more should not be fetched when its wait is over', () => {
    const { store, engine, sent } = setup();
    const release = wantMedia(store, ADA, IMAGE, ORIGINAL);
    fail(engine, sent.at(-1)!.request);
    release();
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    expect(fetches(sent)).toBe(1);
  });

  it('the next view after the wait should fetch media that failed', () => {
    const { store, engine, sent } = setup();
    const release = wantMedia(store, ADA, IMAGE, ORIGINAL);
    fail(engine, sent.at(-1)!.request);
    release();
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    expect(fetches(sent)).toBe(2);
  });

  it('each failure in a row should wait longer before the next fetch', () => {
    const { store, engine, sent } = setup();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    fail(engine, sent.at(-1)!.request);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    fail(engine, sent.at(-1)!.request);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    expect(fetches(sent)).toBe(2);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    expect(fetches(sent)).toBe(3);
  });

  it('a fetch that finally worked should not be fetched again', () => {
    const { store, engine, sent } = setup();
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    fail(engine, sent.at(-1)!.request);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    engine.emit({
      type: 'Finished',
      data: { request: sent.at(-1)!.request, result: { Ok: null } },
    });
    jest.advanceTimersByTime(LONGEST_WAIT_MS);
    wantMedia(store, ADA, IMAGE, ORIGINAL);
    expect(fetches(sent)).toBe(2);
  });

  it('a link card that failed should be fetched again while its card shows', () => {
    const { store, engine, sent } = setup();
    wantLinkPreview(store, ADA, 'https://example.org');
    fail(engine, sent.at(-1)!.request);
    jest.advanceTimersByTime(FIRST_WAIT_MS);
    expect(sent.filter((s) => s.command.type === 'PreviewLink')).toHaveLength(2);
  });
});
