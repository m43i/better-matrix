import { detachedEngine } from '@better-matrix/matrix-native/client';

import { routes } from '../../routes';
import { Store } from '../../state/store';
import { ADA, summary } from '../../test/fixtures';
import { openJoined } from '../links/open';

jest.mock('expo-router', () => ({ router: { navigate: jest.fn(), push: jest.fn() } }));

const { router: navigation } = jest.requireMock<{ router: { navigate: jest.Mock; push: jest.Mock } }>('expo-router');

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
    openJoined(store, { type: 'space', account: GRACE, spaceId: '!guild:x' });
    const grace = store.getState().accounts.find((a) => a.summary.id === GRACE);
    expect([store.getState().active, grace?.scope, navigation.navigate.mock.calls]).toEqual([GRACE, { space: '!guild:x' }, [[routes.home()]]]);
  });

  it('a room joined by another account should open in that account', () => {
    const store = setUp();
    openJoined(store, { type: 'room', account: GRACE, roomId: '!r:x', eventId: null });
    expect([store.getState().active, navigation.push.mock.calls]).toEqual([GRACE, [[routes.room(GRACE, '!r:x', undefined)]]]);
  });
});
