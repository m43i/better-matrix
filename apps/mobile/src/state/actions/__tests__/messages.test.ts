import { detachedEngine } from '@better-matrix/matrix-native/client';

import { ADA, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import {
  listImagePacks,
  locateMessage,
  searchPlaces,
  sendLocation,
  updateScheduled,
} from '../messages';

function setUp() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  const finish = (index: number, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: {
        request: sent[index]!.request,
        result: error === undefined ? { Ok: null } : { Err: error },
      },
    });

  return { engine, store, sent, finish };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('message actions', () => {
  it('should land in the thread the engine says shows a message', async () => {
    const { engine, store, sent, finish } = setUp();
    const located = locateMessage(store, ADA, '!r', '$reply');
    const request = sent[0]!.request;

    engine.emit({
      type: 'MessageLocated',
      data: { request, account: ADA, key: { room_id: '!r', thread: '$root' }, event_id: '$reply' },
    });
    finish(0);
    await expect(located).resolves.toEqual({ room_id: '!r', thread: '$root' });
  });

  it('should hand back the places found', async () => {
    const { engine, store, sent, finish } = setUp();
    const found = searchPlaces(store, 'Tor', 'de');
    const place = { name: 'Tor', geo_uri: 'geo:1,2', map: 'osm:1,2,15' };
    expect(sent[0]!.command).toEqual({
      type: 'SearchPlaces',
      data: { query: 'Tor', language: 'de' },
    });
    engine.emit({ type: 'PlacesFound', data: { request: sent[0]!.request, places: [place] } });
    finish(0);
    await expect(found).resolves.toEqual([place]);
  });

  it('should list a room’s packs in the background', () => {
    const { store, sent } = setUp();
    listImagePacks(store, ADA, '!r');
    expect(sent.map((s) => s.command)).toEqual([
      { type: 'ListImagePacks', data: { account: ADA, room_id: '!r' } },
    ]);
  });

  it('should send a place with what to call it', () => {
    const { store, sent } = setUp();
    sendLocation(store, ADA, { room_id: '!r', thread: null }, 'geo:1,2', 'Home');
    expect(sent[0]!.command).toMatchObject({
      type: 'Send',
      data: { message: { type: 'Location', data: { geo_uri: 'geo:1,2', description: 'Home' } } },
    });
  });

  it('should say when a scheduled message didn’t change', async () => {
    const { store, finish } = setUp();
    updateScheduled(store, ADA, 'd1', 'Cancel');
    finish(0, 'gone');
    await flush();
    expect(store.getState().notices).toHaveLength(1);
  });
});
