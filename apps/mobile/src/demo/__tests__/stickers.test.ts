import type { ImagePack } from '@better-matrix/matrix-native/types';

import { DEMO_GIFS, demoGifPage, editedPacks, editedPlace, packsFor } from '../stickers';
import { demoPacks, GENERAL } from '../fixtures';

const previews = new Map(DEMO_GIFS.map((gif) => [gif.media, `file:///demo/${gif.id}.png`]));

const room = (state_key: string) =>
  ({ type: 'Room', data: { room_id: GENERAL, state_key } }) as const;

describe('demo packs', () => {
  it('should list a room’s packs only in that room, the user’s own everywhere', () => {
    expect(packsFor(demoPacks(), null).map((p) => p.id)).toEqual(['user']);
  });

  it('should make a new room pack under a key of its own, as the engine does', () => {
    const create = { type: 'Create', data: 'Cats' } as const;
    const packs = editedPacks(demoPacks(), room(''), create, 7);

    expect(editedPlace(room(''), create, 7)).toEqual(room('cats-7'));
    expect(packs.find((p) => p.id === `${GENERAL}/cats-7`)?.name).toBe('Cats');
  });

  it('should make a room pack named on its first rename', () => {
    const packs = editedPacks(demoPacks(), room('cats'), { type: 'Rename', data: 'Cats' }, 1);

    expect(packs.find((p) => p.id === `${GENERAL}/cats`)?.name).toBe('Cats');
  });

  it('should add a picked file under a short code made from its name', () => {
    const packs = editedPacks(
      demoPacks(),
      { type: 'User' },
      {
        type: 'AddImages',
        data: [
          {
            source: { type: 'File', data: '/tmp/Happy Cat.png' },
            shortcode: null,
            body: null,
            emoticon: false,
            sticker: true,
          },
        ],
      },
      7,
    );

    const own = packs.find((p): p is ImagePack => p.id === 'user');

    expect(own?.images.at(-1)?.shortcode).toBe('happy_cat');
  });

  it('should use a room’s pack everywhere once asked', () => {
    const packs = editedPacks(demoPacks(), room(''), { type: 'UseEverywhere', data: true }, 1);

    expect(packsFor(packs, null).map((p) => p.id)).toEqual(['user', `${GENERAL}/`]);
  });

  it('should delete a room’s pack', () => {
    expect(editedPacks(demoPacks(), room(''), { type: 'Delete' }, 1).map((p) => p.id)).toEqual([
      'user',
    ]);
  });
});

describe('demo GIFs', () => {
  it('should page through the demo’s GIFs', () => {
    const first = demoGifPage('', 0, previews);
    const second = demoGifPage('', first.next ?? 0, previews);

    expect([first.gifs.length, second.next]).toEqual([4, null]);
  });

  it('should find GIFs by what they show', () => {
    expect(demoGifPage('ship', 0, previews).gifs.map((gif) => gif.id)).toEqual(['ship']);
  });
});
