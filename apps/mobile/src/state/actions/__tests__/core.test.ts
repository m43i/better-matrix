import { detachedEngine } from '@better-matrix/matrix-native/client';

import {
  closeTimeline,
  COLD_ENTRIES,
  coldTimeline,
  cyclePlaybackSpeed,
  loadOlder,
  markRead,
  openTimeline,
  visitRoom,
  WARM_TIMELINES,
} from '../core';
import { timeline } from '../../selectors';
import { Store } from '../../store';
import { roomKey, threadKey } from '../../timeline';
import { ADA, entry, person, roomEventEntry, summary, timelineOf } from '../../../test/fixtures';

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
    expect(sent.at(-1)?.command).toEqual({
      type: 'MarkRead',
      data: { account: ADA, key: room, event_id: '$5', fully_read: false },
    });
  });

  it('should move the fully-read marker once the boundary is in loaded history', () => {
    const { store, engine, sent } = setup();
    engine.emit({
      type: 'ReadBoundary',
      data: { account: ADA, room_id: '!a:x', event_id: '$2', loaded: true },
    });
    markRead(store, ADA, room, '$5');
    expect(sent.at(-1)?.command).toMatchObject({ data: { fully_read: true } });
  });

  it('should never move the room fully-read marker from a thread', () => {
    const { store, engine, sent } = setup();
    engine.emit({
      type: 'ReadBoundary',
      data: { account: ADA, room_id: '!a:x', event_id: null, loaded: false },
    });
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
    engine.emit({
      type: 'Finished',
      data: { request: sent.at(-1)!.request, result: { Err: error } },
    });
    await Promise.resolve();

    return timeline(store.getState(), ADA, room);
  };

  it('a page that failed without HistoryFailed should stop loading and show its error', async () => {
    expect(await failed('the timeline closed')).toMatchObject({
      loadingOlder: false,
      historyError: 'the timeline closed',
    });
  });

  it('a cancelled page should stop loading without an error', async () => {
    expect(await failed('cancelled')).toMatchObject({ loadingOlder: false, historyError: null });
  });

  it('a failed page should be asked for again', async () => {
    const { store, engine, sent } = setup();
    loadOlder(store, ADA, room);
    engine.emit({
      type: 'Finished',
      data: { request: sent.at(-1)!.request, result: { Err: 'offline' } },
    });
    await Promise.resolve();
    loadOlder(store, ADA, room);
    expect(sent.filter((s) => s.command.type === 'LoadOlder')).toHaveLength(2);
  });
});

it('showing a room should record a visit independently of its timeline subscription', () => {
  const { store, sent } = setup();
  visitRoom(store, ADA, room.room_id);
  expect(sent.map((s) => s.command)).toEqual([
    { type: 'VisitRoom', data: { account: ADA, room_id: room.room_id } },
  ]);
});

describe('cyclePlaybackSpeed', () => {
  it('should step the speed for every clip and come back round', () => {
    const { store } = setup();

    const speeds = [1, 2, 3].map(() => {
      cyclePlaybackSpeed(store);

      return store.getState().settings.playbackSpeed;
    });

    expect(speeds).toEqual([1.5, 2, 1]);
  });
});

describe('closed timelines', () => {
  const entries = (count: number) => Array.from({ length: count }, (_, i) => entry(`$${i}`));

  /** Opens room `i`, fills it with `count` entries to its start as the engine would, and closes it. */
  function visit({ store, engine }: ReturnType<typeof setup>, i: number, count = 50) {
    const key = roomKey(`!${i}:x`);
    openTimeline(store, ADA, key);
    engine.emit({
      type: 'TimelineChanged',
      data: { account: ADA, key, diffs: [{ type: 'Reset', data: entries(count) }] },
    });
    engine.emit({ type: 'HistoryLoaded', data: { account: ADA, key, reached_start: true } });
    closeTimeline(store, ADA, key);
  }

  const entriesOf = (app: ReturnType<typeof setup>, i: number) =>
    timeline(app.store.getState(), ADA, roomKey(`!${i}:x`)).entries;

  it('the timelines the engine keeps warm should keep all their entries', () => {
    const app = setup();

    for (let i = 0; i < WARM_TIMELINES; i++) visit(app, i);
    expect(entriesOf(app, 0)).toHaveLength(50);
  });

  it('a timeline closed longer ago than the engine keeps should keep only its newest entries', () => {
    const app = setup();

    for (let i = 0; i <= WARM_TIMELINES; i++) visit(app, i);
    expect(entriesOf(app, 0).map((e) => e.id)).toEqual(
      entries(50)
        .slice(-COLD_ENTRIES)
        .map((e) => e.id),
    );
    expect(timeline(app.store.getState(), ADA, roomKey('!0:x')).reachedStart).toBe(false);
    expect(entriesOf(app, 1)).toHaveLength(50);
  });

  it('a timeline opened again should count as closed only from its next close', () => {
    const app = setup();

    for (let i = 0; i < WARM_TIMELINES; i++) visit(app, i);
    visit(app, 0);
    visit(app, WARM_TIMELINES);
    expect(entriesOf(app, 0)).toHaveLength(50);
    expect(entriesOf(app, 1)).toHaveLength(COLD_ENTRIES);
  });

  it('a cold timeline should keep its newest message for the room list', () => {
    const grace = person('@grace:x');

    const events = Array.from({ length: COLD_ENTRIES + 5 }, (_, i) =>
      roomEventEntry(`e${i}`, grace, { type: 'Joined' }),
    );

    const cold = coldTimeline(timelineOf([...entries(10), ...events]));
    expect(cold.entries[0]?.id).toBe('$9');
    expect(cold.entries).toHaveLength(COLD_ENTRIES + 6);
  });

  const MEMBERS = {
    people: [],
    complete: true,
    can_invite: false,
    own_level: 0,
    can_kick: false,
    can_ban: false,
    can_set_roles: false,
  };

  /** Room `i` as screens over it fetched it: its members, threads, files and pins. */
  function detailed({ engine }: ReturnType<typeof setup>, i: number) {
    const room_id = `!${i}:x`;
    engine.emitAll([
      { type: 'Members', data: { request: null, account: ADA, room_id, members: MEMBERS } },
      { type: 'Threads', data: { account: ADA, room_id, threads: [] } },
      { type: 'Files', data: { account: ADA, room_id, files: [] } },
      { type: 'PinnedMessages', data: { account: ADA, room_id, messages: [] } },
    ]);
  }

  const detailsOf = (app: ReturnType<typeof setup>, i: number) => {
    const account = app.store.getState().accounts[0]!;
    const room = `!${i}:x`;

    return [
      room in account.members,
      room in account.threads,
      room in account.files,
      room in account.pins,
    ];
  };

  it('a room whose timelines all went cold should forget its members, threads, files and pins', () => {
    const app = setup();
    detailed(app, 0);

    for (let i = 0; i <= WARM_TIMELINES; i++) visit(app, i);
    expect(detailsOf(app, 0)).toEqual([false, false, false, false]);
  });

  it('a room whose thread is still warm should keep its details', () => {
    const app = setup();
    detailed(app, 0);
    visit(app, 0);
    openTimeline(app.store, ADA, threadKey('!0:x', '$root'));
    closeTimeline(app.store, ADA, threadKey('!0:x', '$root'));

    for (let i = 1; i < WARM_TIMELINES; i++) visit(app, i);
    expect(entriesOf(app, 0)).toHaveLength(COLD_ENTRIES);
    expect(detailsOf(app, 0)).toEqual([true, true, true, true]);
  });

  it('a room on screen should keep its details when its thread goes cold', () => {
    const app = setup();
    detailed(app, 0);
    openTimeline(app.store, ADA, roomKey('!0:x'));
    openTimeline(app.store, ADA, threadKey('!0:x', '$root'));
    closeTimeline(app.store, ADA, threadKey('!0:x', '$root'));

    for (let i = 1; i <= WARM_TIMELINES; i++) visit(app, i);
    expect(detailsOf(app, 0)).toEqual([true, true, true, true]);
  });

  it('a short timeline should stay as it is', () => {
    const short = timelineOf(entries(COLD_ENTRIES), { reachedStart: true });
    expect(coldTimeline(short)).toBe(short);
  });
});
