import { detachedEngine } from '@better-matrix/matrix-native/client';

import type { PackImage } from '@better-matrix/matrix-native/types';

import { ADA, message, summary } from '../../../test/fixtures';
import { composer, startReply } from '../../composer';
import { mxcMedia } from '../../messages';
import { Store } from '../../store';
import { roomKey, threadKey } from '../../timeline';
import {
  listImagePacks,
  locateMessage,
  searchPlaces,
  sendLocation,
  sendSticker,
  updateScheduled,
} from '../messages';

const STICKER: PackImage = {
  shortcode: 'wave',
  body: 'Wave',
  url: 'mxc://x/wave',
  media: mxcMedia('mxc://x/wave'),
  emoticon: false,
  sticker: true,
  dimensions: [256, 256],
  mimetype: 'image/png',
};

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
    await expect(located).resolves.toEqual({
      key: { room_id: '!r', thread: '$root' },
      eventId: '$reply',
    });
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

  it('a sticker should answer the message the composer replies to', () => {
    const { store, sent } = setUp();
    startReply(store, ADA, roomKey('!r'), message('$q'));
    sendSticker(store, ADA, roomKey('!r'), STICKER);
    expect(sent[0]!.command).toMatchObject({ type: 'Send', data: { reply_to: '$q' } });
  });

  it('a sticker should end the composer’s reply once sent', () => {
    const { store } = setUp();
    startReply(store, ADA, roomKey('!r'), message('$q'));
    sendSticker(store, ADA, roomKey('!r'), STICKER);
    expect(composer(store.getState(), ADA, roomKey('!r')).replyTo).toBeNull();
  });

  it('a sticker should answer nothing when the composer replies to nothing', () => {
    const { store, sent } = setUp();
    sendSticker(store, ADA, roomKey('!r'), STICKER);
    expect(sent[0]!.command).toMatchObject({ type: 'Send', data: { reply_to: null } });
  });

  it('a place should answer the message the composer replies to, and end the reply', () => {
    const { store, sent } = setUp();
    startReply(store, ADA, roomKey('!r'), message('$q'));
    sendLocation(store, ADA, roomKey('!r'), 'geo:1,2', null);
    expect([
      sent[0]!.command,
      composer(store.getState(), ADA, roomKey('!r')).replyTo,
    ]).toMatchObject([{ type: 'Send', data: { reply_to: '$q' } }, null]);
  });

  it('a sticker from a thread’s composer should go into the thread, answering its reply', () => {
    const { store, sent } = setUp();
    const thread = threadKey('!r', '$root');
    startReply(store, ADA, thread, message('$q'));
    sendSticker(store, ADA, thread, STICKER);
    expect(sent[0]!.command).toMatchObject({ data: { key: thread, reply_to: '$q' } });
  });

  it('a sticker should leave another timeline’s reply as it was', () => {
    const { store } = setUp();
    startReply(store, ADA, roomKey('!r'), message('$q'));
    sendSticker(store, ADA, threadKey('!r', '$root'), STICKER);
    expect(composer(store.getState(), ADA, roomKey('!r')).replyTo?.event_id).toBe('$q');
  });

  it('should say when a scheduled message didn’t change', async () => {
    const { store, finish } = setUp();
    updateScheduled(store, ADA, 'd1', 'Cancel');
    finish(0, 'gone');
    await flush();
    expect(store.getState().notices).toHaveLength(1);
  });
});
