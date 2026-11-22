import { detachedEngine } from '@better-matrix/matrix-native/client';

import { routes } from '../../routes';
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
    expect(navigation.push.mock.calls).toEqual([[routes.thread(ADA, '!r:x', '$root', '$reply')]]);
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
      'opened',
      [[routes.thread(ADA, '!r:x', '$root', '$q')]],
    ]);
  });

  it('a room message quoted in a thread should open in the room', async () => {
    expect(await located({ thread: null }, '$root')).toEqual([
      'opened',
      [[routes.room(ADA, '!r:x', '$q')]],
    ]);
  });

  it('a quoted message that would show in this timeline should open nothing', async () => {
    expect(await located({ thread: '$root' }, '$root')).toEqual(['missing', []]);
  });

  it('a quoted message the engine can’t find should open nothing', async () => {
    expect(await located('unknown', null)).toEqual(['missing', []]);
  });

  it('a quoted message found once its timeline left the screen should open nothing', async () => {
    expect(await located({ thread: '$root' }, null, true)).toEqual(['stale', []]);
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
      'stale',
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
