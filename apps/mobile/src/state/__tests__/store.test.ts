import { detachedEngine, type EngineEvent } from '@better-matrix/matrix-native/client';

import { fetchMedia } from '../actions/core';
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

  it('media whose file could not be written should be fetched again', () => {
    const { store, engine, sent } = setup();
    fetchMedia(store, ADA, IMAGE, { type: 'Original' });
    const request = sent.at(-1)!.request;
    engine.emitAll([
      { type: 'MediaFailed', data: { request, error: 'disk full' } },
      { type: 'Finished', data: { request, result: { Ok: null } } },
    ]);
    fetchMedia(store, ADA, IMAGE, { type: 'Original' });
    expect(sent.filter((s) => s.command.type === 'FetchMedia')).toHaveLength(2);
  });

  it('media fetched well should not be fetched again', () => {
    const { store, engine, sent } = setup();
    fetchMedia(store, ADA, IMAGE, { type: 'Original' });
    const request = sent.at(-1)!.request;
    engine.emitAll([
      {
        type: 'MediaFile',
        data: { request, media: IMAGE, size: { type: 'Original' }, path: '/cache/a.png' },
      },
      { type: 'Finished', data: { request, result: { Ok: null } } },
    ]);
    fetchMedia(store, ADA, IMAGE, { type: 'Original' });
    expect(sent.filter((s) => s.command.type === 'FetchMedia')).toHaveLength(1);
  });
});
