import { detachedEngine } from '@better-matrix/matrix-native/client';

import { routes } from '../../routes';
import { Store } from '../../state/store';
import { ADA, summary } from '../../test/fixtures';
import { openElsewhere, openJoined } from '../links/open';

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
  const located = async (where: { thread: string | null } | 'unknown', current: string | null) => {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });

    const opened = openElsewhere(
      store,
      ADA,
      { room_id: '!r:x', thread: current },
      '$q',
      navigation,
    );

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
      true,
      [[routes.thread(ADA, '!r:x', '$root', '$q')]],
    ]);
  });

  it('a room message quoted in a thread should open in the room', async () => {
    expect(await located({ thread: null }, '$root')).toEqual([
      true,
      [[routes.room(ADA, '!r:x', '$q')]],
    ]);
  });

  it('a quoted message that would show in this timeline should open nothing', async () => {
    expect(await located({ thread: '$root' }, '$root')).toEqual([false, []]);
  });

  it('a quoted message the engine can’t find should open nothing', async () => {
    expect(await located('unknown', null)).toEqual([false, []]);
  });
});
