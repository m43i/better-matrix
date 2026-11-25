// What the composer's sheets send besides text (stickers, places) and what they look up for it
// (image packs, places by name), a message's edit history, and where a message is for a jump.
// A sticker or place answers what the composer answers, as text would, and ends that reply.

import type { AccountId, PackImage, Place, TimelineKey } from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import { composer, updateComposer } from '../composer';
import { addNotice } from '../reducer';
import { CommandFailed } from '../requests';
import { rememberSticker } from '../stickers';
import type { Store } from '../store';

/** Lists the custom emoji and sticker packs usable in a room, in the background. */
export function listImagePacks(store: Store, account: AccountId, roomId: string | null): void {
  store.send({ type: 'ListImagePacks', data: { account, room_id: roomId } }, { quiet: true });
}

/** The message `key`'s composer answers, taken from it: the next send answers it instead. */
function takeReply(store: Store, account: AccountId, key: TimelineKey): string | null {
  const replyTo = composer(store.getState(), account, key).replyTo?.event_id ?? null;

  if (replyTo !== null) updateComposer(store, account, key, (c) => ({ ...c, replyTo: null }));

  return replyTo;
}

/**
 * Sends a pack's image as a sticker, answering what the timeline's composer answers; the picker
 * shows it first among the account's recent stickers from then on.
 */
export function sendSticker(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  image: PackImage,
): void {
  const replyTo = takeReply(store, account, key);
  store.update((s) => ({
    ...s,
    settings: {
      ...s.settings,
      recentStickers: {
        ...s.settings.recentStickers,
        [account]: rememberSticker(s.settings.recentStickers[account] ?? [], image),
      },
    },
  }));
  store.send({
    type: 'Send',
    data: { account, key, reply_to: replyTo, message: { type: 'Sticker', data: image } },
  });
}

/** Sends a place (a `geo:` URI and what to call it), answering what the composer answers. */
export function sendLocation(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  geoUri: string,
  description: string | null,
): void {
  const replyTo = takeReply(store, account, key);
  store.send({
    type: 'Send',
    data: {
      account,
      key,
      reply_to: replyTo,
      message: { type: 'Location', data: { geo_uri: geoUri, description } },
    },
  });
}

/** Finds places by name on OpenStreetMap; only when the user asks, never as they type. */
export async function searchPlaces(
  store: Store,
  query: string,
  language: string | null,
): Promise<Place[]> {
  let places: Place[] = [];
  await store.run({ type: 'SearchPlaces', data: { query, language } }, (event) => {
    if (event.type === 'PlacesFound') places = event.data.places;
  });

  return places;
}

/** Lists the versions of an edited message (they arrive in the account's `edits`). */
export function listEdits(
  store: Store,
  account: AccountId,
  roomId: string,
  eventId: string,
): Promise<void> {
  return store.run({ type: 'ListEdits', data: { account, room_id: roomId, event_id: eventId } });
}

/** Where a message shows (the room's timeline, or the thread it is in) and its ID there. */
export interface Located {
  key: TimelineKey;
  /** The message: the one an edit edits, when asked for an edit. */
  eventId: string;
}

/** Which timeline shows a message: the room's, or the thread it is in. */
export async function locateMessage(
  store: Store,
  account: AccountId,
  roomId: string,
  eventId: string,
): Promise<Located> {
  let located: Located = { key: { room_id: roomId, thread: null }, eventId };
  await store.run(
    { type: 'LocateMessage', data: { account, room_id: roomId, event_id: eventId } },
    (event) => {
      if (event.type === 'MessageLocated') {
        located = { key: event.data.key, eventId: event.data.event_id };
      }
    },
  );

  return located;
}

/** Lists the account's scheduled messages. */
export function listScheduled(store: Store, account: AccountId): void {
  store.send({ type: 'ListScheduled', data: { account } }, { quiet: true });
}

/** Sends a scheduled message now, or cancels it; a failure says so. */
export function updateScheduled(
  store: Store,
  account: AccountId,
  delayId: string,
  action: 'Cancel' | 'SendNow',
): void {
  store
    .run({ type: 'UpdateScheduled', data: { account, delay_id: delayId, action } })
    .catch((cause: unknown) => {
      if (cause instanceof CommandFailed && cause.cancelled) return;
      const error = cause instanceof Error ? cause.message : String(cause);
      store.update((s) => addNotice(s, t('mobile.composer.scheduled_failed', { error }), account));
    });
}
