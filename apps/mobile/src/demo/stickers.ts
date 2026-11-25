// The demo's image packs and GIF search, answered as the engine would: packs listed for a room
// (the user's own, the room's, those used everywhere), edits applied to them, and a few GIFs
// "found on GIPHY" whose previews are the demo's own pictures. Pure, so it can be tested.

import type {
  FoundGif,
  GifPage,
  ImagePack,
  PackEdit,
  PackImage,
  PackPlace,
} from '@better-matrix/matrix-native/types';

import { mxcMedia } from '../state/messages';
import { shortcodeOf } from '../state/stickers';

/** The packs usable in `roomId` (or anywhere), as `ListImagePacks` lists them. */
export function packsFor(packs: readonly ImagePack[], roomId: string | null): ImagePack[] {
  return packs.filter(
    (pack) => pack.place.type === 'User' || pack.everywhere || pack.place.data.room_id === roomId,
  );
}

function samePlace(a: PackPlace, b: PackPlace): boolean {
  if (a.type === 'User' || b.type === 'User') return a.type === b.type;

  return a.data.room_id === b.data.room_id && a.data.state_key === b.data.state_key;
}

function newPack(place: PackPlace, name: string): ImagePack {
  return {
    id: place.type === 'User' ? 'user' : `${place.data.room_id}/${place.data.state_key}`,
    name,
    avatar: null,
    avatar_url: null,
    images: [],
    place,
    editable: true,
    everywhere: false,
  };
}

/** A free short code like `wanted`, numbered when taken. */
function freeCode(images: readonly PackImage[], wanted: string): string {
  const taken = new Set(images.map((image) => image.shortcode));

  if (!taken.has(wanted)) return wanted;

  for (let n = 2; ; n++) if (!taken.has(`${wanted}_${n}`)) return `${wanted}_${n}`;
}

/**
 * The pack `edit` changes: `place`, or for `Create` a new room pack's, under a state key the
 * engine would pick (from its name, never used before: `serial` makes it so here).
 */
export function editedPlace(place: PackPlace, edit: PackEdit, serial: number): PackPlace {
  if (edit.type !== 'Create' || place.type !== 'Room') return place;

  return {
    type: 'Room',
    data: { room_id: place.data.room_id, state_key: `${shortcodeOf(edit.data)}-${serial}` },
  };
}

/** `packs` after `edit` to the pack at `place`; `serial` names uploaded pictures. */
export function editedPacks(
  packs: readonly ImagePack[],
  asked: PackPlace,
  edit: PackEdit,
  serial: number,
): ImagePack[] {
  const place = editedPlace(asked, edit, serial);
  const found = packs.find((pack) => samePlace(pack.place, place));

  if (edit.type === 'Delete') return packs.filter((pack) => pack !== found);
  const pack = found ?? newPack(place, '');
  let changed: ImagePack = pack;

  switch (edit.type) {
    case 'Rename':
    case 'Create':
      changed = { ...pack, name: edit.data.trim() };
      break;
    case 'SetAvatar':
      changed = {
        ...pack,
        avatar_url: edit.data,
        avatar: pack.images.find((image) => image.url === edit.data)?.media ?? null,
      };
      break;
    case 'AddImages': {
      const images = [...pack.images];

      edit.data.forEach((added, index) => {
        const url = `mxc://example.org/demo-added-${serial}-${index}`;

        const name =
          added.source.type === 'File'
            ? (added.source.data.split('/').pop() ?? '').replace(/\.[^.]*$/, '')
            : added.source.data.name;

        const shortcode = freeCode(images, added.shortcode ?? shortcodeOf(name));
        images.push({
          shortcode,
          body: added.body ?? shortcode,
          url,
          media: added.source.type === 'Media' ? added.source.data.media : mxcMedia(url),
          emoticon: added.emoticon,
          sticker: added.sticker,
          dimensions: added.source.type === 'Media' ? added.source.data.dimensions : null,
          mimetype: null,
        });
      });
      changed = { ...pack, images };
      break;
    }

    case 'ChangeImage':
      changed = {
        ...pack,
        images: pack.images.map((image) =>
          image.shortcode === edit.data.shortcode
            ? {
                ...image,
                shortcode: edit.data.image.shortcode,
                body: edit.data.image.body.trim() || edit.data.image.shortcode,
                emoticon: edit.data.image.emoticon,
                sticker: edit.data.image.sticker,
              }
            : image,
        ),
      };
      break;
    case 'RemoveImage':
      changed = { ...pack, images: pack.images.filter((image) => image.shortcode !== edit.data) };
      break;
    case 'UseEverywhere':
      changed = { ...pack, everywhere: edit.data };
      break;
  }

  return found ? packs.map((known) => (known === found ? changed : known)) : [...packs, changed];
}

/** The demo's GIFs: what each shows, and the demo media its preview is drawn from. */
export const DEMO_GIFS: ReadonlyArray<{ id: string; title: string; media: string }> = [
  { id: 'wave', title: 'Waving hello', media: mxcMedia('mxc://example.org/sticker-wave') },
  { id: 'ship', title: 'Ship it', media: mxcMedia('mxc://example.org/sticker-ship') },
  { id: 'coffee', title: 'Coffee break', media: mxcMedia('mxc://example.org/sticker-coffee') },
  { id: 'party', title: 'Party time', media: mxcMedia('mxc://example.org/emote-party') },
  { id: 'ok', title: 'OK', media: mxcMedia('mxc://example.org/emote-ok') },
];

/** The demo previews' size: square. */
const SQUARE: [number, number] = [200, 200];

/** GIFs per demo page. */
const DEMO_PAGE = 4;

/**
 * A page of the demo's GIFs matching `query` (all for a blank one) from `offset`, each preview
 * from `previews` (by demo media; GIFs without one are left out).
 */
export function demoGifPage(
  query: string,
  offset: number,
  previews: ReadonlyMap<string, string>,
): GifPage {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);

  const found: FoundGif[] = DEMO_GIFS.filter(
    (gif) => words.length === 0 || words.some((word) => gif.title.toLowerCase().includes(word)),
  ).flatMap((gif) => {
    const preview = previews.get(gif.media);

    return preview
      ? [
          {
            id: gif.id,
            title: gif.title,
            url: `https://giphy.com/gifs/demo-${gif.id}`,
            preview,
            preview_size: SQUARE,
            sent_ping: null,
          },
        ]
      : [];
  });

  const gifs = found.slice(offset, offset + DEMO_PAGE);
  const next = offset + DEMO_PAGE < found.length ? offset + DEMO_PAGE : null;

  return { gifs, next };
}
