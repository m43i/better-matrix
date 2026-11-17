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

  return `https://matrix.to/#/${encodeURIComponent(roomId)}/${encodeURIComponent(eventId)}?via=${encodeURIComponent(server)}`;
}

export function translationUrl(body: string, language: string): string {
  return `https://translate.google.com/?sl=auto&tl=${encodeURIComponent(language)}&text=${encodeURIComponent(body)}&op=translate`;
}
