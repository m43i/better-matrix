import type {
  Attachment,
  FoundGif,
  ImagePack,
  PackImage,
} from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import {
  canSaveSticker,
  gifPageShown,
  gifSearchFailed,
  gifRows,
  gifTrouble,
  packGroups,
  packTitle,
  RECENT_STICKERS,
  rememberSticker,
  savedSticker,
  shortcodeOf,
  stickerSections,
  validShortcode,
} from '../stickers';

const ROOM = '!r:x';

function image(code: string, use: { sticker?: boolean; emoticon?: boolean } = {}): PackImage {
  return {
    shortcode: code,
    body: `${code} body`,
    url: `mxc://x/${code}`,
    media: `m-${code}`,
    emoticon: use.emoticon ?? false,
    sticker: use.sticker ?? true,
    dimensions: null,
    mimetype: null,
  };
}

function pack(id: string, images: PackImage[], change: Partial<ImagePack> = {}): ImagePack {
  return {
    id,
    name: id,
    avatar: null,
    avatar_url: null,
    images,
    place: { type: 'Room', data: { room_id: ROOM, state_key: id } },
    editable: true,
    everywhere: false,
    ...change,
  };
}

const own = (images: PackImage[]) => pack('user', images, { name: '', place: { type: 'User' } });

function gif(id: string, size: [number, number] | null): FoundGif {
  return {
    id,
    title: id,
    url: `https://giphy.com/gifs/${id}`,
    preview: `https://media.giphy.com/${id}.gif`,
    preview_size: size,
    sent_ping: null,
  };
}

describe('stickerSections', () => {
  it('should show the stickers sent lately first, then each pack’s', () => {
    const packs = [own([image('cat')]), pack('room', [image('wave'), image('ship')])];
    const sections = stickerSections(packs, [image('ship')], '');

    expect(sections.map((s) => [s.title, s.stickers.map((i) => i.shortcode)])).toEqual([
      [t('mobile.stickers.recent'), ['ship']],
      [t('mobile.stickers.yours'), ['cat']],
      ['room', ['wave', 'ship']],
    ]);
  });

  it('should leave out pictures usable only as emoji', () => {
    const sections = stickerSections(
      [own([image('ok', { sticker: false, emoticon: true })])],
      [],
      '',
    );

    expect(sections).toEqual([]);
  });

  it('should filter by short code or description while something is typed', () => {
    const packs = [pack('room', [image('wave'), { ...image('ship'), body: 'Ship it' }])];
    const sections = stickerSections(packs, [image('wave')], 'SHIP');

    expect(sections.map((s) => s.stickers.map((i) => i.shortcode))).toEqual([['ship']]);
  });

  it('should offer a recent sticker as its pack has it now', () => {
    const packs = [pack('room', [{ ...image('wave'), body: 'A new description' }])];
    const sections = stickerSections(packs, [image('wave')], '');

    expect(sections[0]?.stickers[0]?.body).toBe('A new description');
  });

  it('should drop a recent sticker no pack here holds any more', () => {
    const sections = stickerSections([pack('room', [image('wave')])], [image('gone')], '');

    expect(sections.map((s) => s.key)).toEqual(['room']);
  });
});

describe('rememberSticker', () => {
  it('should put a sticker sent again first, once', () => {
    const recent = rememberSticker([image('a'), image('b')], image('b'));

    expect(recent.map((i) => i.shortcode)).toEqual(['b', 'a']);
  });

  it('should keep only the latest few', () => {
    const many = Array.from({ length: RECENT_STICKERS }, (_, i) => image(`s${i}`));

    expect(rememberSticker(many, image('new'))).toHaveLength(RECENT_STICKERS);
  });
});

describe('packGroups', () => {
  it('should sort packs into the user’s own, the room’s and other rooms’', () => {
    const elsewhere = pack('far', [], {
      place: { type: 'Room', data: { room_id: '!far:x', state_key: '' } },
      everywhere: true,
    });

    const groups = packGroups([own([]), pack('room', []), elsewhere], ROOM);

    expect([
      groups.own?.id,
      groups.room.map((p) => p.id),
      groups.elsewhere.map((p) => p.id),
    ]).toEqual(['user', ['room'], ['far']]);
  });
});

describe('packTitle', () => {
  it('should call the user’s own pack without a name their stickers', () => {
    expect(packTitle(own([]))).toBe(t('mobile.stickers.yours'));
  });

  it('should call a named pack by its name', () => {
    expect(packTitle(pack('Cats', []))).toBe('Cats');
  });
});

describe('short codes', () => {
  it('should refuse a short code the spec’s grammar leaves out', () => {
    expect(
      ['a b', 'a:b', '', 'cat.v2', 'cat/blue', 'grüße', 'a'.repeat(101)].map(validShortcode),
    ).toEqual([false, false, false, false, false, false, false]);
  });

  it('should take a short code of ASCII letters, digits, dashes and underscores', () => {
    expect(validShortcode('Cat_wave-2')).toBe(true);
  });

  it('should make a short code from a name as the engine does', () => {
    expect([shortcodeOf('Happy Cat!'), shortcodeOf('Grüße'), shortcodeOf('…')]).toEqual([
      'happy_cat',
      'gr_e',
      'sticker',
    ]);
  });
});

describe('saving a sent picture as a sticker', () => {
  const sent: Attachment = {
    kind: 'Image',
    name: 'Ship it',
    size: null,
    media: 'm-ship',
    thumbnail: null,
    dimensions: [256, 256],
    audio: null,
    sticker: true,
  };

  it('should add a sticker as a sticker, described as it was', () => {
    expect(savedSticker(sent)).toEqual({
      source: { type: 'Media', data: { media: 'm-ship', name: 'Ship it', dimensions: [256, 256] } },
      shortcode: null,
      body: 'Ship it',
      emoticon: false,
      sticker: true,
    });
  });

  it('should not describe a picture by its file name', () => {
    expect(savedSticker({ ...sent, name: 'IMG_1.jpg', sticker: false }).body).toBeNull();
  });

  it('should offer it for pictures only', () => {
    expect([canSaveSticker(sent), canSaveSticker({ ...sent, kind: 'File' })]).toEqual([
      true,
      false,
    ]);
  });
});

describe('GIFs', () => {
  it('should lay GIFs out row by row in GIPHY’s order, each row as tall as its tallest', () => {
    const rows = gifRows(
      [gif('tall', [100, 200]), gif('wide', [200, 100]), gif('c', null)],
      2,
      100,
    );

    expect(rows.map((row) => row.map((placed) => [placed.gif.id, placed.height]))).toEqual([
      [
        ['tall', 200],
        ['wide', 200],
      ],
      [['c', 100]],
    ]);
  });

  it('should add a next page to the search shown, each GIF once', () => {
    const shown = { query: 'cat', gifs: [gif('a', null)], next: 24 };
    const page = { gifs: [gif('a', null), gif('b', null)], next: 48 };

    expect(gifPageShown(shown, 'cat', 24, page)).toEqual({
      query: 'cat',
      gifs: [gif('a', null), gif('b', null)],
      next: 48,
    });
  });

  it('should leave nothing of the search before to page through when a new search fails', () => {
    const shown = { query: 'cat', gifs: [gif('a', null)], next: 24 };

    expect(gifSearchFailed(shown, 'dog', 0)).toEqual({ query: 'dog', gifs: [], next: null });
  });

  it('should keep what shows when a later page fails', () => {
    const shown = { query: 'cat', gifs: [gif('a', null)], next: 24 };

    expect(gifSearchFailed(shown, 'cat', 24)).toBe(shown);
  });

  it('should not add a later page of another search to the GIFs shown', () => {
    const shown = { query: 'cat', gifs: [gif('a', null)], next: 24 };

    expect(gifPageShown(shown, 'dog', 24, { gifs: [gif('d', null)], next: 48 })).toBe(shown);
  });

  it('should replace what shows with a new search’s first page', () => {
    const shown = { query: 'cat', gifs: [gif('a', null)], next: 24 };

    expect(gifPageShown(shown, 'dog', 0, { gifs: [gif('d', null)], next: 24 })).toEqual({
      query: 'dog',
      gifs: [gif('d', null)],
      next: 24,
    });
  });

  it('should tell a refused key from a missing one and other failures', () => {
    expect([
      gifTrouble('GIPHY refused the API key'),
      gifTrouble('GIF search needs a GIPHY API key'),
      gifTrouble('GIPHY didn’t answer: offline'),
    ]).toEqual(['refused', 'no_key', 'failed']);
  });
});
