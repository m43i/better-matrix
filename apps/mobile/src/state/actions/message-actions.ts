// Shared message utilities; commands keep source account and timeline explicit.
import type { AccountId, TimelineKey } from '@better-matrix/matrix-native/types';
import type { Store } from '../store';

export function setPinned(
  store: Store,
  account: AccountId,
  roomId: string,
  eventId: string,
  pinned: boolean,
): void {
  store.send({ type: 'SetPinned', data: { account, room_id: roomId, event_id: eventId, pinned } });
}

export function listPins(store: Store, account: AccountId, roomId: string): Promise<void> {
  return store.run({ type: 'ListPinnedMessages', data: { account, room_id: roomId } });
}

/** Saves a message for the user alone, or stops saving it; room summaries follow through sync. */
export function setSaved(
  store: Store,
  account: AccountId,
  roomId: string,
  eventId: string,
  saved: boolean,
): void {
  store.send({ type: 'SetSaved', data: { account, room_id: roomId, event_id: eventId, saved } });
}

/** Lists every message the account saved, into `AccountState.saved`. */
export function listSaved(store: Store, account: AccountId): Promise<void> {
  return store.run({ type: 'ListSavedMessages', data: { account } });
}

export function forwardMessage(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  eventId: string,
  roomId: string,
): Promise<void> {
  return store.run({ type: 'Forward', data: { account, key, event_id: eventId, room_id: roomId } });
}

/** Matches the core's matrix.to builder; the account's server routes even v12 room IDs. */
export function messageLink(roomId: string, eventId: string, account: AccountId): string {
  const server = account.slice(account.indexOf(':') + 1);

  return `https://matrix.to/#/${encodeComponent(roomId)}/${encodeComponent(eventId)}?via=${encodeComponent(server)}`;
}

export function translationUrl(body: string, language: string): string {
  return `https://translate.google.com/?sl=auto&tl=${encodeComponent(language)}&text=${encodeComponent(body)}&op=translate`;
}

/** The WHATWG form encoding Rust's url::form_urlencoded uses. */
function encodeComponent(text: string): string {
  return encodeURIComponent(text)
    .replace(/[!'()~]/g, (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`)
    .replace(/%20/g, '+');
}
