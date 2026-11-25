import { detachedEngine, type DetachedEngine } from '@better-matrix/matrix-native/client';
import type { Attachment, Command, FoundGif, PackImage } from '@better-matrix/matrix-native/types';

import { t } from '../../../i18n';
import { ADA, message, roomSummary, summary } from '../../../test/fixtures';
import { updateComposer } from '../../composer';
import { Store } from '../../store';
import { roomKey } from '../../timeline';
import { sendSticker } from '../messages';
import {
  createRoomPack,
  editPack,
  saveToStickers,
  searchGifs,
  sendGif,
  keepGiphyKeyIn,
  saveGiphyKey,
  showGiphyKey,
} from '../stickers';

const ROOM = '!lab:example.org';

function setUp(): DetachedEngine & { store: Store } {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  detached.engine.emit({
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms: [roomSummary(ROOM)], spaces: [], invitations: [] } },
  });

  return { ...detached, store };
}

function last({ sent }: DetachedEngine): { request: number; command: Command } {
  const entry = sent[sent.length - 1];

  if (!entry) throw new Error('nothing was sent');

  return entry;
}

function finish(engine: DetachedEngine['engine'], request: number, error?: string): void {
  engine.emit({
    type: 'Finished',
    data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
  });
}

/** Lets the awaited request's answer through. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const notices = (app: { store: Store }) => app.store.getState().notices.map((n) => n.message);

const GIF: FoundGif = {
  id: 'a1',
  title: 'A cat waves',
  url: 'https://giphy.com/gifs/cat-a1',
  preview: 'https://media.giphy.com/media/a1/200w.gif?cid=x',
  preview_size: [200, 150],
  sent_ping: null,
};

const STICKER: PackImage = {
  shortcode: 'wave',
  body: 'Waving',
  url: 'mxc://x/wave',
  media: 'm-wave',
  emoticon: false,
  sticker: true,
  dimensions: null,
  mimetype: null,
};

describe('sending GIFs', () => {
  it('should send a GIF as what the composer answers, and end the reply', () => {
    const app = setUp();
    const key = roomKey(ROOM);
    updateComposer(app.store, ADA, key, (c) => ({
      ...c,
      replyTo: message('$q'),
    }));
    sendGif(app.store, ADA, key, GIF);

    expect(last(app).command).toEqual({
      type: 'Send',
      data: { account: ADA, key, reply_to: '$q', message: { type: 'Gif', data: GIF } },
    });
  });

  it('should hand back the page GIPHY found', async () => {
    const app = setUp();
    const found = searchGifs(app.store, 'key', 'cat', 0, 'en');
    const { request, command } = last(app);
    app.engine.emit({
      type: 'GifsFound',
      data: { request, query: 'cat', offset: 0, page: { gifs: [GIF], next: 24 } },
    });
    finish(app.engine, request);

    expect(command).toEqual({
      type: 'SearchGifs',
      data: { key: 'key', query: 'cat', offset: 0, language: 'en' },
    });
    await expect(found).resolves.toEqual({ gifs: [GIF], next: 24 });
  });

  it('should keep a GIPHY key without its spaces, and forget a blank one', () => {
    const app = setUp();
    showGiphyKey(app.store, '  abc  ');
    const kept = app.store.getState().giphyKey;
    showGiphyKey(app.store, ' ');

    expect([kept, app.store.getState().giphyKey]).toEqual(['abc', null]);
  });
});

describe('keeping the GIPHY key', () => {
  /** A vault that answers each write when told to. */
  function vault() {
    const kept: Array<string | null> = [];
    const pending: Array<{ resolve: () => void; reject: (error: Error) => void }> = [];

    const write = (key: string | null) =>
      new Promise<void>((resolve, reject) => {
        pending.push({
          resolve: () => {
            kept.push(key);
            resolve();
          },
          reject,
        });
      });

    keepGiphyKeyIn({ save: (key) => write(key), remove: () => write(null) });

    return { kept, pending };
  }

  afterEach(() => keepGiphyKeyIn(null));

  it('should show a key as kept only once the secure store has it', async () => {
    const app = setUp();
    const { pending } = vault();
    const saved = saveGiphyKey(app.store, ' abc ');
    await settle();
    const before = app.store.getState().giphyKey;
    pending[0]?.resolve();

    await expect(saved).resolves.toBe(true);
    expect([before, app.store.getState().giphyKey]).toEqual([null, 'abc']);
  });

  it('should keep showing the key when removing it fails, and say so', async () => {
    const app = setUp();
    showGiphyKey(app.store, 'abc');
    const { pending } = vault();
    const removed = saveGiphyKey(app.store, null);
    await settle();
    pending[0]?.reject(new Error('locked'));

    await expect(removed).resolves.toBe(false);
    expect(app.store.getState().giphyKey).toBe('abc');
    expect(notices(app)).toEqual([t('mobile.stickers.giphy_remove_failed', { error: 'locked' })]);
  });

  it('should keep the last of two quick changes', async () => {
    const app = setUp();
    const { kept, pending } = vault();
    const first = saveGiphyKey(app.store, 'one');
    const second = saveGiphyKey(app.store, 'two');
    await settle();
    pending[0]?.resolve();
    await first;
    await settle();
    pending[1]?.resolve();
    await second;

    expect([kept, app.store.getState().giphyKey]).toEqual([['one', 'two'], 'two']);
  });

  it('should show the key kept when a later change fails', async () => {
    const app = setUp();
    const { pending } = vault();
    const first = saveGiphyKey(app.store, 'one');
    const second = saveGiphyKey(app.store, 'two');
    await settle();
    pending[0]?.resolve();
    await first;
    await settle();
    pending[1]?.reject(new Error('locked'));
    await second;

    expect(app.store.getState().giphyKey).toBe('one');
  });
});

describe('stickers sent lately', () => {
  it('should remember a sticker the account sent', () => {
    const app = setUp();
    sendSticker(app.store, ADA, roomKey(ROOM), STICKER);

    expect(app.store.getState().settings.recentStickers[ADA]).toEqual([STICKER]);
  });
});

describe('editing packs', () => {
  it('should list every pack list again once a change is made', async () => {
    const app = setUp();
    app.engine.emit({
      type: 'ImagePacks',
      data: { account: ADA, room_id: ROOM, packs: [], may_add: false },
    });
    const done = editPack(app.store, ADA, { type: 'User' }, { type: 'Rename', data: 'Mine' });
    finish(app.engine, last(app).request);
    await done;

    expect(last(app).command).toEqual({
      type: 'ListImagePacks',
      data: { account: ADA, room_id: ROOM },
    });
  });

  it('should say why a change failed', async () => {
    const app = setUp();
    const done = editPack(app.store, ADA, { type: 'User' }, { type: 'RemoveImage', data: 'x' });
    finish(app.engine, last(app).request, 'offline');

    await expect(done).resolves.toBe(false);
    expect(notices(app)).toEqual([t('mobile.stickers.edit_failed', { error: 'offline' })]);
  });

  it('should list the packs again after a change failed, part of it may have gone through', async () => {
    const app = setUp();
    app.engine.emit({
      type: 'ImagePacks',
      data: { account: ADA, room_id: ROOM, packs: [], may_add: false },
    });
    const done = editPack(app.store, ADA, { type: 'User' }, { type: 'Rename', data: 'Mine' });
    finish(app.engine, last(app).request, 'offline');
    await done;

    expect(last(app).command).toEqual({
      type: 'ListImagePacks',
      data: { account: ADA, room_id: ROOM },
    });
  });

  it('should add a sent picture to the user’s own pack and say so', async () => {
    const app = setUp();

    const sent: Attachment = {
      kind: 'Image',
      name: 'Ship it',
      size: null,
      media: 'm-ship',
      thumbnail: null,
      dimensions: null,
      audio: null,
      sticker: true,
    };

    const done = saveToStickers(app.store, ADA, sent);
    const { request, command } = last(app);
    finish(app.engine, request);
    await done;
    await settle();

    expect(command).toMatchObject({
      type: 'EditImagePack',
      data: {
        account: ADA,
        pack: { type: 'User' },
        edit: { type: 'AddImages', data: [{ source: { type: 'Media' }, sticker: true }] },
      },
    });
    expect(notices(app)).toEqual([t('mobile.stickers.added')]);
  });

  it('should ask the engine to make a room pack, and give the place it made', async () => {
    const app = setUp();
    const made = createRoomPack(app.store, ADA, ROOM, ' Cats ');
    const { request, command } = last(app);
    const place = { type: 'Room' as const, data: { room_id: ROOM, state_key: 'cats-1a2b3c' } };
    app.engine.emit({ type: 'ImagePackEdited', data: { request, account: ADA, pack: place } });
    finish(app.engine, request);

    await expect(made).resolves.toEqual(place);
    expect(command).toEqual({
      type: 'EditImagePack',
      data: {
        account: ADA,
        pack: { type: 'Room', data: { room_id: ROOM, state_key: '' } },
        edit: { type: 'Create', data: 'Cats' },
      },
    });
  });
});
