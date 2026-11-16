import { detachedEngine } from '@better-matrix/matrix-native/client';

import { loadOlder, markRead, openTimeline } from '../core';
import { timeline } from '../../selectors';
import { Store } from '../../store';
import { ADA, summary } from '../../../test/fixtures';

function setup() {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  return { ...detached, store };
}

const room = { room_id: '!a:x', thread: null };

describe('markRead', () => {
  it('should not move the fully-read marker before the read boundary is known', () => {
    const { store, sent } = setup();
    markRead(store, ADA, room, '$5');
    expect(sent.at(-1)?.command).toEqual({ type: 'MarkRead', data: { account: ADA, key: room, event_id: '$5', fully_read: false } });
  });

  it('should move the fully-read marker once the boundary is in loaded history', () => {
    const { store, engine, sent } = setup();
    engine.emit({ type: 'ReadBoundary', data: { account: ADA, room_id: '!a:x', event_id: '$2', loaded: true } });
    markRead(store, ADA, room, '$5');
    expect(sent.at(-1)?.command).toMatchObject({ data: { fully_read: true } });
  });

  it('should never move the room fully-read marker from a thread', () => {
    const { store, engine, sent } = setup();
    engine.emit({ type: 'ReadBoundary', data: { account: ADA, room_id: '!a:x', event_id: null, loaded: false } });
    markRead(store, ADA, { room_id: '!a:x', thread: '$root' }, '$5');
    expect(sent.at(-1)?.command).toMatchObject({ data: { fully_read: false } });
  });
});

describe('opening a timeline', () => {
  it('history should not be asked for while the timeline opens', () => {
    const { store, sent } = setup();
    openTimeline(store, ADA, room);
    loadOlder(store, ADA, room);
    expect(sent.map((s) => s.command.type)).toEqual(['OpenTimeline']);
  });

  it('history should be asked for once the timeline opened', async () => {
    const { store, engine, sent } = setup();
    openTimeline(store, ADA, room);
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Ok: null } } });
    await Promise.resolve();
    loadOlder(store, ADA, room);
    expect(sent.map((s) => s.command.type)).toEqual(['OpenTimeline', 'LoadOlder']);
  });
});

describe('loadOlder', () => {
  const failed = async (error: string) => {
    const { store, engine, sent } = setup();
    loadOlder(store, ADA, room);
    engine.emit({ type: 'Finished', data: { request: sent.at(-1)!.request, result: { Err: error } } });
    await Promise.resolve();
    return timeline(store.getState(), ADA, room);
  };

  it('a page that failed without HistoryFailed should stop loading and show its error', async () => {
    expect(await failed('the timeline closed')).toMatchObject({ loadingOlder: false, historyError: 'the timeline closed' });
  });

  it('a cancelled page should stop loading without an error', async () => {
    expect(await failed('cancelled')).toMatchObject({ loadingOlder: false, historyError: null });
  });

  it('a failed page should be asked for again', async () => {
    const { store, engine, sent } = setup();
    loadOlder(store, ADA, room);
    engine.emit({ type: 'Finished', data: { request: sent.at(-1)!.request, result: { Err: 'offline' } } });
    await Promise.resolve();
    loadOlder(store, ADA, room);
    expect(sent.filter((s) => s.command.type === 'LoadOlder')).toHaveLength(2);
  });
});
