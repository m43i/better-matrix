import { detachedEngine } from '@better-matrix/matrix-native/client';

import { routes } from '../../routes';
import {
  openSeek,
  quoteHere,
  quoteTapped,
  seekEnded,
  seekStep,
  sought,
  soughtReach,
} from '../../state/seek';
import { Store } from '../../state/store';
import { ADA, roomSummary, summary } from '../../test/fixtures';
import { beginJump, openElsewhere, openJoined, openPermalink } from '../links/open';

const navigation = { navigate: jest.fn(), push: jest.fn() };

const GRACE = '@grace:example.org';

function setUp() {
  const { engine } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  engine.emit({ type: 'AccountAdded', data: summary(GRACE) });
  store.update((s) => ({ ...s, active: ADA }));

  return store;
}

beforeEach(() => jest.clearAllMocks());

describe('openJoined', () => {
  it('a space joined by another account should show that account on Home, scoped to the space', () => {
    const store = setUp();
    openJoined(store, { type: 'space', account: GRACE, spaceId: '!guild:x' }, navigation);
    const grace = store.getState().accounts.find((a) => a.summary.id === GRACE);
    expect([store.getState().active, grace?.scope, navigation.navigate.mock.calls]).toEqual([
      GRACE,
      { space: '!guild:x' },
      [[routes.home()]],
    ]);
  });

  it('a room joined by another account should open in that account', () => {
    const store = setUp();
    openJoined(store, { type: 'room', account: GRACE, roomId: '!r:x', eventId: null }, navigation);
    expect([store.getState().active, navigation.push.mock.calls]).toEqual([
      GRACE,
      [[routes.room(GRACE, '!r:x', undefined)]],
    ]);
  });
});

describe('openJoined at a message', () => {
  it('should open a reply in its thread', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    openJoined(
      store,
      { type: 'room', account: ADA, roomId: '!r:x', eventId: '$reply' },
      navigation,
    );
    const request = sent[0]!.request;

    engine.emit({
      type: 'MessageLocated',
      data: {
        request,
        account: ADA,
        key: { room_id: '!r:x', thread: '$root' },
        event_id: '$reply',
      },
    });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([
      [routes.thread(ADA, '!r:x', '$root', '$reply', 'located')],
    ]);
  });

  it('a link to an edit should open the message it edits', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    openJoined(store, { type: 'room', account: ADA, roomId: '!r:x', eventId: '$edit' }, navigation);
    const request = sent[0]!.request;

    engine.emit({
      type: 'MessageLocated',
      data: {
        request,
        account: ADA,
        key: { room_id: '!r:x', thread: null },
        event_id: '$original',
      },
    });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([
      [routes.room(ADA, '!r:x', '$original', 'located')],
    ]);
  });

  it('should open nothing once the user left the screen that asked', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    let shown = true;
    const route = { type: 'room' as const, account: ADA, roomId: '!r:x', eventId: '$m' };
    openJoined(store, route, navigation, () => shown);
    shown = false;
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Ok: null } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([]);
  });

  it('should open only the latest of two jumps, whichever answer comes last', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    openJoined(store, { type: 'room', account: ADA, roomId: '!r:x', eventId: '$a' }, navigation);
    openJoined(store, { type: 'room', account: ADA, roomId: '!r:x', eventId: '$b' }, navigation);
    engine.emit({ type: 'Finished', data: { request: sent[1]!.request, result: { Err: 'no' } } });
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'no' } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([[routes.room(ADA, '!r:x', '$b')]]);
  });

  it('should open the room at the message when the engine can’t tell', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    openJoined(store, { type: 'room', account: ADA, roomId: '!r:x', eventId: '$m' }, navigation);
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'no' } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([[routes.room(ADA, '!r:x', '$m')]]);
  });
});

describe('openElsewhere', () => {
  const located = async (
    where: { thread: string | null } | 'unknown',
    current: string | null,
    leave = false,
  ) => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    let shown = true;

    const opened = openElsewhere(
      store,
      ADA,
      { room_id: '!r:x', thread: current },
      '$q',
      beginJump(() => shown),
      navigation,
    );

    if (leave) shown = false;
    const request = sent[0]!.request;

    if (where !== 'unknown')
      engine.emit({
        type: 'MessageLocated',
        data: { request, account: ADA, key: { room_id: '!r:x', ...where }, event_id: '$q' },
      });
    engine.emit({
      type: 'Finished',
      data: { request, result: where === 'unknown' ? { Err: 'no' } : { Ok: null } },
    });

    return [await opened, navigation.push.mock.calls];
  };

  it('a quoted reply in a thread should open in its thread', async () => {
    expect(await located({ thread: '$root' }, null)).toEqual([
      { type: 'opened' },
      [[routes.thread(ADA, '!r:x', '$root', '$q', 'located')]],
    ]);
  });

  it('a quoted reply opened in its thread should carry that the engine located it there', async () => {
    const [, calls] = await located({ thread: '$root' }, null);
    expect(calls).toMatchObject([[{ params: { eventId: '$q', reach: 'located' } }]]);
  });

  it('a room message quoted in a thread should open in the room', async () => {
    expect(await located({ thread: null }, '$root')).toEqual([
      { type: 'opened' },
      [[routes.room(ADA, '!r:x', '$q', 'located')]],
    ]);
  });

  it('a quoted message that shows in this timeline should open nothing, but be looked for here', async () => {
    expect(await located({ thread: '$root' }, '$root')).toEqual([
      { type: 'here', eventId: '$q' },
      [],
    ]);
  });

  it('a quoted thread root, from the room, should be looked for in the room', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });

    const outcome = openElsewhere(
      store,
      ADA,
      { room_id: '!r:x', thread: null },
      '$q',
      beginJump(),
      navigation,
    );

    const request = sent[0]!.request;
    engine.emit({
      type: 'MessageLocated',
      data: { request, account: ADA, key: { room_id: '!r:x', thread: '$q' }, event_id: '$q' },
    });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });

    expect([await outcome, navigation.push.mock.calls]).toEqual([
      { type: 'here', eventId: '$q' },
      [],
    ]);
  });

  it('a quoted edit in this timeline should be looked for as the message it edits', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    // The quote's search paged back without finding `$edit`, and asked the engine.
    const gaveUp = seekEnded(quoteTapped(openSeek(null), '$edit'), '$edit');

    const outcome = openElsewhere(
      store,
      ADA,
      { room_id: '!r:x', thread: null },
      '$edit',
      beginJump(),
      navigation,
    );

    const request = sent[0]!.request;
    engine.emit({
      type: 'MessageLocated',
      data: {
        request,
        account: ADA,
        key: { room_id: '!r:x', thread: null },
        event_id: '$original',
      },
    });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
    const answer = await outcome;
    const seek = answer.type === 'here' ? quoteHere(gaveUp, '$edit', answer.eventId) : gaveUp;

    const step = (loaded: boolean) =>
      seekStep({
        quote: true,
        wanted: true,
        loaded,
        busy: false,
        atStart: false,
        pages: 0,
        budget: 3,
        reach: soughtReach(seek),
      });

    // With `$original` loaded it scrolls there at once, else it pages back for it.
    expect([sought(seek), step(true), step(false), navigation.push.mock.calls]).toEqual([
      '$original',
      'scroll',
      'page',
      [],
    ]);
  });

  it('a quoted message the engine can’t find should open nothing', async () => {
    expect(await located('unknown', null)).toEqual([{ type: 'missing' }, []]);
  });

  it('a quoted message found once its timeline left the screen should open nothing', async () => {
    expect(await located({ thread: '$root' }, null, true)).toEqual([{ type: 'stale' }, []]);
  });
});

describe('jumps begun at the tap', () => {
  it('a quote paged back for after a later jump began should open nothing, nor ask', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    const quote = beginJump();
    openJoined(store, { type: 'room', account: ADA, roomId: '!r:x', eventId: null }, navigation);

    const outcome = await openElsewhere(
      store,
      ADA,
      { room_id: '!r:x', thread: null },
      '$q',
      quote,
      navigation,
    );

    expect([outcome, sent.length, navigation.push.mock.calls]).toEqual([
      { type: 'stale' },
      0,
      [[routes.room(ADA, '!r:x', undefined)]],
    ]);
  });

  it('a link looked up when a quote was tapped since should open nothing', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    openJoined(store, { type: 'room', account: ADA, roomId: '!r:x', eventId: '$m' }, navigation);
    beginJump();
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'no' } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([]);
  });
});

describe('openPermalink', () => {
  it('a permalink tapped in a message should open nothing once its screen left', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    engine.emit({
      type: 'RoomsUpdated',
      data: { account: ADA, list: { rooms: [roomSummary('!r:x')], spaces: [], invitations: [] } },
    });
    let shown = true;
    openPermalink(store, 'https://matrix.to/#/!r:x/$m', ADA, () => shown, navigation);
    shown = false;
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'no' } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([]);
  });

  it('a permalink tapped in a message should open the room at it while its screen is on show', async () => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    engine.emit({
      type: 'RoomsUpdated',
      data: { account: ADA, list: { rooms: [roomSummary('!r:x')], spaces: [], invitations: [] } },
    });
    openPermalink(store, 'https://matrix.to/#/!r:x/$m', ADA, () => true, navigation);
    engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'no' } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigation.push.mock.calls).toEqual([[routes.room(ADA, '!r:x', '$m')]]);
  });
});
