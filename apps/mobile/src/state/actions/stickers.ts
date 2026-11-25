// Stickers and GIFs: sending a GIF found on GIPHY, searching GIPHY with the user's own key, and
// managing image packs (the user's own and rooms'): renaming, adding pictures (files picked on
// the phone, or a sticker or picture someone sent), changing and removing them, using a room's
// pack everywhere, making and deleting room packs. A change re-lists every pack list the account
// holds, since the user's own pack shows in each; a failure becomes a notice.

import type {
  AccountId,
  Attachment,
  FoundGif,
  GifPage,
  NewPackImage,
  PackEdit,
  PackPlace,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import { composer, updateComposer } from '../composer';
import { addNotice } from '../reducer';
import { CommandFailed } from '../requests';
import { account as accountOf } from '../selectors';
import { savedSticker } from '../stickers';
import type { Store } from '../store';

/** Sends a GIF found on GIPHY (its link), answering what the timeline's composer answers. */
export function sendGif(store: Store, account: AccountId, key: TimelineKey, gif: FoundGif): void {
  const replyTo = composer(store.getState(), account, key).replyTo?.event_id ?? null;

  if (replyTo !== null) updateComposer(store, account, key, (c) => ({ ...c, replyTo: null }));
  store.send({
    type: 'Send',
    data: { account, key, reply_to: replyTo, message: { type: 'Gif', data: gif } },
  });
}

/**
 * A page of GIFs from GIPHY: those matching `query` from `offset`, or trending ones for a blank
 * query. Rejects with `CommandFailed` (see `gifTrouble`).
 */
export async function searchGifs(
  store: Store,
  key: string,
  query: string,
  offset: number,
  language: string | null,
): Promise<GifPage> {
  let page: GifPage = { gifs: [], next: null };
  await store.run({ type: 'SearchGifs', data: { key, query, offset, language } }, (event) => {
    if (event.type === 'GifsFound') page = event.data.page;
  });

  return page;
}

/** Where the GIPHY key is kept: the secure store (`engine/giphyKey.ts`), or a test's. */
export interface KeyVault {
  save(key: string): Promise<void>;
  remove(): Promise<void>;
}

let vault: KeyVault | null = null;

/** The writes on their way, one after another, so the last asked for is the last kept. */
let writes: Promise<unknown> = Promise.resolve();

/** Keeps the GIPHY key in `next` from now on (none: changes stay in this run). */
export function keepGiphyKeyIn(next: KeyVault | null): void {
  vault = next;
}

/** Shows `key` as the one kept, without keeping it anywhere: the key read at launch, demos. */
export function showGiphyKey(store: Store, key: string | null): void {
  const kept = key?.trim() ? key.trim() : null;
  store.update((s) => ({ ...s, giphyKey: kept }));
}

/**
 * Keeps the user's GIPHY API key in the vault (blank removes it); the app shows the change only
 * once it is kept. Writes go in order, so the key shown is always the last one kept: of quick
 * changes the latest wins, and one that fails leaves the one before it showing. Whether it was
 * kept; a failure says why.
 */
export async function saveGiphyKey(store: Store, key: string | null): Promise<boolean> {
  const kept = key?.trim() ? key.trim() : null;
  const target = vault;

  const write = writes.then(() =>
    target === null ? undefined : kept === null ? target.remove() : target.save(kept),
  );

  writes = write.catch(() => undefined);

  try {
    await write;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);

    const notice =
      kept === null ? 'mobile.stickers.giphy_remove_failed' : 'mobile.stickers.giphy_save_failed';

    store.update((s) => addNotice(s, t(notice, { error: message })));

    return false;
  }

  showGiphyKey(store, kept);

  return true;
}

/** Lists again every pack list the account holds, in the background. */
export function relistPacks(store: Store, account: AccountId): void {
  const lists = Object.keys(accountOf(store.getState(), account)?.packs ?? {});

  for (const list of lists.length > 0 ? lists : ['']) {
    store.send(
      { type: 'ListImagePacks', data: { account, room_id: list === '' ? null : list } },
      { quiet: true },
    );
  }
}

/** Changes a pack; whether it worked. A failure says why. */
export async function editPack(
  store: Store,
  account: AccountId,
  pack: PackPlace,
  edit: PackEdit,
): Promise<boolean> {
  return (await changePack(store, account, pack, edit)) !== null;
}

/**
 * Changes a pack; the pack changed (a new room pack's place, for `Create`), or `null` when it
 * didn't work. A failure says why.
 */
async function changePack(
  store: Store,
  account: AccountId,
  pack: PackPlace,
  edit: PackEdit,
): Promise<PackPlace | null> {
  let changed: PackPlace = pack;

  try {
    await store.run({ type: 'EditImagePack', data: { account, pack, edit } }, (event) => {
      if (event.type === 'ImagePackEdited') changed = event.data.pack;
    });
  } catch (error) {
    if (error instanceof CommandFailed && error.cancelled) return null;
    const message = error instanceof Error ? error.message : String(error);
    store.update((s) =>
      addNotice(s, t('mobile.stickers.edit_failed', { error: message }), account),
    );
    // Part of it may have gone through (one copy of a pack kept under both names): what the
    // server holds now shows.
    relistPacks(store, account);

    return null;
  }

  relistPacks(store, account);

  return changed;
}

/** Adds pictures picked on the phone to a pack, as stickers. */
export function addStickerFiles(
  store: Store,
  account: AccountId,
  pack: PackPlace,
  paths: readonly string[],
): Promise<boolean> {
  const images: NewPackImage[] = paths.map((path) => ({
    source: { type: 'File', data: path },
    shortcode: null,
    body: null,
    emoticon: false,
    sticker: true,
  }));

  return editPack(store, account, pack, { type: 'AddImages', data: images });
}

/** Adds a sticker or picture someone sent to the user's own pack, and says so. */
export async function saveToStickers(
  store: Store,
  account: AccountId,
  attachment: Attachment,
): Promise<void> {
  const added = await editPack(
    store,
    account,
    { type: 'User' },
    { type: 'AddImages', data: [savedSticker(attachment)] },
  );

  if (added) store.update((s) => addNotice(s, t('mobile.stickers.added'), account));
}

/**
 * Makes a pack in a room, named `name`, under a state key the engine picks (never one used
 * before). Gives its place, or `null` when it couldn't be made.
 */
export function createRoomPack(
  store: Store,
  account: AccountId,
  roomId: string,
  name: string,
): Promise<PackPlace | null> {
  const room: PackPlace = { type: 'Room', data: { room_id: roomId, state_key: '' } };

  return changePack(store, account, room, { type: 'Create', data: name.trim() });
}
