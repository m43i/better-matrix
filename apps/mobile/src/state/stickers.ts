// Stickers and GIFs, derived for the picker and the pack manager: the picker's sections (recently
// sent, then each pack's stickers, filtered by what is typed), the packs to manage grouped by
// where they are kept, recently sent stickers, and GIFs laid out in rows. Pure: the screens and
// `actions/stickers.ts` use them.

import type {
  AccountId,
  Attachment,
  FoundGif,
  GifPage,
  ImagePack,
  PackImage,
} from '@better-matrix/matrix-native/types';

import { t } from '../i18n';
import { account as accountOf } from './selectors';
import type { State } from './types';

/** How many recently sent stickers each account keeps. */
export const RECENT_STICKERS = 16;

/** Where GIPHY gives out API keys: each user brings their own. */
export const GIPHY_KEYS_URL = 'https://developers.giphy.com/dashboard/';

/** The packs' key in an account's lists for `roomId`: `''` for the account's own, anywhere. */
export function packsKey(roomId: string | null | undefined): string {
  return roomId ?? '';
}

/** The packs listed for `roomId` (or anywhere), the user's own first. */
export function listedPacks(
  state: State,
  account: AccountId,
  roomId: string | null | undefined,
): ImagePack[] {
  const data = accountOf(state, account);

  return data?.packs[packsKey(roomId)] ?? (roomId ? (data?.packs[''] ?? []) : []);
}

/** Whether the account may add a pack to `roomId`, as last listed. */
export function mayAddPack(state: State, account: AccountId, roomId: string | null): boolean {
  return roomId !== null && (accountOf(state, account)?.packsMayAdd[roomId] ?? false);
}

/** What a pack is called: its name, or for the user's own pack without one, "Your stickers". */
export function packTitle(pack: ImagePack): string {
  return pack.place.type === 'User' && pack.name.trim() === ''
    ? t('mobile.stickers.yours')
    : pack.name;
}

/** Whether `image` matches what was typed: by short code or description, any case. */
export function stickerMatches(image: PackImage, query: string): boolean {
  const q = query.trim().toLowerCase();

  return (
    q === '' || image.shortcode.toLowerCase().includes(q) || image.body.toLowerCase().includes(q)
  );
}

export interface StickerSection {
  key: string;
  title: string;
  stickers: PackImage[];
}

/**
 * The picker's sections: the stickers sent lately (still in a pack here, while nothing is typed),
 * then each pack's stickers, all filtered by `query`; empty sections are left out.
 */
export function stickerSections(
  packs: readonly ImagePack[],
  recent: readonly PackImage[],
  query: string,
): StickerSection[] {
  const sections: StickerSection[] = [];

  const usable = new Map(
    packs.flatMap((pack) => pack.images.filter((i) => i.sticker).map((i) => [i.url, i] as const)),
  );

  // Each as its pack has it now: a description changed since goes with it.
  const lately = recent.flatMap((image) => {
    const current = usable.get(image.url);

    return current ? [current] : [];
  });

  if (query.trim() === '' && lately.length > 0)
    sections.push({ key: 'recent', title: t('mobile.stickers.recent'), stickers: lately });

  for (const pack of packs) {
    const stickers = pack.images.filter((image) => image.sticker && stickerMatches(image, query));

    if (stickers.length > 0) sections.push({ key: pack.id, title: packTitle(pack), stickers });
  }

  return sections;
}

/** `recent` with `image` first, each picture once, at most `RECENT_STICKERS`. */
export function rememberSticker(recent: readonly PackImage[], image: PackImage): PackImage[] {
  return [image, ...recent.filter((known) => known.url !== image.url)].slice(0, RECENT_STICKERS);
}

export interface PackGroups {
  /** The user's own pack. */
  own: ImagePack | null;
  /** The packs of the room the manager was opened for. */
  room: ImagePack[];
  /** Packs of other rooms the user uses everywhere. */
  elsewhere: ImagePack[];
}

/** The packs to manage, by where they are kept. */
export function packGroups(packs: readonly ImagePack[], roomId: string | null): PackGroups {
  const groups: PackGroups = { own: null, room: [], elsewhere: [] };

  for (const pack of packs) {
    if (pack.place.type === 'User') groups.own ??= pack;
    else if (pack.place.data.room_id === roomId) groups.room.push(pack);
    else groups.elsewhere.push(pack);
  }

  return groups;
}

/** The new image a received sticker or picture becomes in the user's pack. */
export function savedSticker(attachment: Attachment) {
  return {
    source: {
      type: 'Media' as const,
      data: {
        media: attachment.media ?? '',
        name: attachment.name,
        dimensions: attachment.dimensions,
      },
    },
    shortcode: null,
    // A sticker's name describes it; a picture's is a file name.
    body: attachment.sticker ? attachment.name : null,
    emoticon: false,
    sticker: true,
  };
}

/** Whether a message's attachment can become one of the user's stickers. */
export function canSaveSticker(attachment: Attachment | null): boolean {
  return attachment !== null && attachment.kind === 'Image' && attachment.media !== null;
}

/** What went wrong with a GIF search, from the engine's error. */
export type GifTrouble = 'no_key' | 'refused' | 'failed';

/** The kind of a failed GIF search, from the engine's message. */
export function gifTrouble(message: string): GifTrouble {
  if (message.includes('refused the API key')) return 'refused';

  if (message.includes('needs a GIPHY API key')) return 'no_key';

  return 'failed';
}

/** The GIFs the GIF tab shows: what they were found for, and where its next page starts. */
export interface GifResults {
  query: string;
  gifs: FoundGif[];
  next: number | null;
}

/** Nothing found yet. */
export const NO_GIFS: GifResults = { query: '', gifs: [], next: null };

/**
 * The GIFs shown once a page arrives for `query` from `offset`: a first page replaces what shows,
 * a later one adds to it (each GIF once) only if it continues the search shown.
 */
export function gifPageShown(
  shown: GifResults,
  query: string,
  offset: number,
  page: GifPage,
): GifResults {
  if (offset === 0) return { query, gifs: page.gifs, next: page.next };

  if (query !== shown.query) return shown;
  const known = new Set(shown.gifs.map((gif) => gif.id));

  return {
    query,
    gifs: [...shown.gifs, ...page.gifs.filter((gif) => !known.has(gif.id))],
    next: page.next,
  };
}

/**
 * The GIFs shown once a search for `query` from `offset` failed: a new search's failure leaves
 * nothing of the search before it to show or page through; a later page's keeps what shows.
 */
export function gifSearchFailed(shown: GifResults, query: string, offset: number): GifResults {
  return offset === 0 ? { ...NO_GIFS, query } : shown;
}

/** A GIF placed in the grid, and how tall it shows. */
export interface PlacedGif {
  gif: FoundGif;
  height: number;
}

/**
 * GIFs in rows of `count`, each `width` wide, in GIPHY's order (as GIPHY asks, and as a screen
 * reader goes): a row is as tall as its tallest GIF at that width (square when unknown), each
 * fitting its tile.
 */
export function gifRows(gifs: readonly FoundGif[], count: number, width: number): PlacedGif[][] {
  const perRow = Math.max(1, count);
  const rows: PlacedGif[][] = [];

  for (let start = 0; start < gifs.length; start += perRow) {
    const row = gifs.slice(start, start + perRow);

    const height = Math.max(
      ...row.map((gif) => {
        const [w, h] = gif.preview_size ?? [1, 1];

        return Math.round(w > 0 && h > 0 ? (width * h) / w : width);
      }),
    );

    rows.push(row.map((gif) => ({ gif, height })));
  }

  return rows;
}

/**
 * Whether `shortcode` may name a new or changed picture, as Matrix 1.19's grammar has it: 1 to
 * 100 ASCII letters, digits, `-` and `_`. Pictures already named otherwise still show.
 */
export function validShortcode(shortcode: string): boolean {
  return /^[A-Za-z0-9_-]{1,100}$/.test(shortcode);
}

/**
 * A short code made from a name, as the engine makes one: ASCII letters and digits kept (lower
 * case), anything else `_`, short enough to take a number after it.
 */
export function shortcodeOf(name: string): string {
  const made = [...name.toLowerCase()]
    .map((c) => (/[a-z0-9-]/.test(c) ? c : '_'))
    .join('')
    .split('_')
    .filter(Boolean)
    .join('_')
    .slice(0, 88);

  return made === '' ? 'sticker' : made;
}
