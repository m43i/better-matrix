// Permalinks: looking up the room a link names (its preview), joining it, and finding the
// person a link names. The link screen (app/link.tsx) runs them for the account it acts for.

import type { AccountId, Person, RoomPreview } from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import { addNotice } from '../reducer';
import { CommandFailed } from '../requests';
import type { Store } from '../store';

/** What the room `room` (an ID or alias) looks like from outside; rejects when no server describes it. */
export async function previewRoom(
  store: Store,
  account: AccountId,
  room: string,
  via: string[],
): Promise<RoomPreview> {
  let preview: RoomPreview | null = null;
  await store.run({ type: 'PreviewRoom', data: { account, room, via } }, (event) => {
    if (event.type === 'RoomPreview') preview = event.data.preview;
  });

  if (preview === null) throw new Error('the engine sent no preview');

  return preview;
}

/** Joins `room` through `via`; resolves to its room ID. A failure is told in the desktop's words. */
export async function joinRoom(
  store: Store,
  account: AccountId,
  room: string,
  via: string[],
): Promise<string> {
  let roomId: string | null = null;

  try {
    await store.run({ type: 'JoinRoom', data: { account, room_id: room, via } }, (event) => {
      if (event.type === 'RoomJoined') roomId = event.data.room_id;
    });
  } catch (error) {
    if (!(error instanceof CommandFailed)) throw error;
    const failed = error;

    if (!failed.cancelled)
      store.update((s) => addNotice(s, t('notice.join', { error: failed.message })));
    throw error;
  }

  if (roomId === null) throw new Error('the engine joined no room');

  return roomId;
}

/**
 * The person `userId` names, with their profile from their server (the directory offers a full
 * user ID first, ADR 0008); `null` when it can't be found.
 */
export async function findPerson(
  store: Store,
  account: AccountId,
  userId: string,
): Promise<Person | null> {
  let found: Person | null = null;
  await store.run({ type: 'SearchPeople', data: { account, term: userId } }, (event) => {
    if (event.type === 'PeopleFound')
      found = event.data.people.find((p) => p.id === userId) ?? null;
  });

  return found;
}
