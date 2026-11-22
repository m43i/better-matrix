import { detachedEngine } from '@better-matrix/matrix-native/client';

import { routes } from '../../../routes';
import { parsePermalink } from '../../../state/permalinks';
import { Store } from '../../../state/store';
import { ADA, roomSummary, summary } from '../../../test/fixtures';
import { beginJump } from '../../links/open';
import { openSearchLink } from '../searchLink';

const navigation = { navigate: jest.fn(), push: jest.fn() };

beforeEach(() => jest.clearAllMocks());

/** An account in `!r:x`, and the link to its message `$m`, pasted into the search. */
function setUp() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  engine.emit({
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms: [roomSummary('!r:x')], spaces: [], invitations: [] } },
  });
  const link = parsePermalink('https://matrix.to/#/!r:x/$m');

  if (link === null) throw new Error('the permalink should parse');
  // The results list's own guard, which picking the link takes away.
  const results = { shown: true };

  /** The engine located `$m` in the room. */
  const answer = async () => {
    const request = sent[0]!.request;
    engine.emit({
      type: 'MessageLocated',
      data: { request, account: ADA, key: { room_id: '!r:x', thread: null }, event_id: '$m' },
    });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
    await new Promise((resolve) => setTimeout(resolve, 0));
  };

  const close = () => {
    results.shown = false;
  };

  return { store, link, results, answer, close };
}

describe('openSearchLink', () => {
  it('a message link picked in search should open once the engine answers, though the search closed', async () => {
    const { store, link, results, answer, close } = setUp();
    openSearchLink(store, link, ADA, () => true, close, navigation);
    await answer();
    expect([results.shown, navigation.push.mock.calls]).toEqual([
      false,
      [[routes.room(ADA, '!r:x', '$m', 'located')]],
    ]);
  });

  it('a message link picked in search should open nothing once the screen hosting the search left', async () => {
    const { store, link, answer, close } = setUp();
    let host = true;
    openSearchLink(store, link, ADA, () => host, close, navigation);
    host = false;
    await answer();
    expect(navigation.push.mock.calls).toEqual([]);
  });

  it('a message link picked in search should open nothing once a later jump began', async () => {
    const { store, link, answer, close } = setUp();
    openSearchLink(store, link, ADA, () => true, close, navigation);
    beginJump();
    await answer();
    expect(navigation.push.mock.calls).toEqual([]);
  });
});
