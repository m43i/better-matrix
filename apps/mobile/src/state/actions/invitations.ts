// Answering an invitation, as the desktop does (ADR 0008): Accept joins (`Command::JoinRoom`),
// Decline leaves without forgetting the room (`Command::LeaveRoom`). Both wait for the engine;
// a failure keeps the invitation so it can be tried again.

import type { AccountId, Invitation } from '@better-matrix/matrix-native/types';

import { CommandFailed } from '../requests';
import { updateAccount } from '../reducer';
import type { Store } from '../store';

/** How an answer ended: the room joined, declined, or why it failed. */
export type InvitationAnswer =
  | { kind: 'joined'; roomId: string }
  | { kind: 'declined' }
  | { kind: 'failed'; error: string | null };

/** The invitation to `roomId` waiting for `account`, if there is one. */
export function invitation(
  store: Store,
  account: AccountId,
  roomId: string,
): Invitation | undefined {
  return store
    .getState()
    .accounts.find((a) => a.summary.id === account)
    ?.invitations.find((invite) => invite.room_id === roomId);
}

/** Accepts or declines; the invitation leaves the list once the engine says it is done. */
export async function answerInvitation(
  store: Store,
  account: AccountId,
  roomId: string,
  accept: boolean,
): Promise<InvitationAnswer> {
  let joined = roomId;

  try {
    if (accept) {
      await store.run(
        { type: 'JoinRoom', data: { account, room_id: roomId, via: [] } },
        (event) => {
          if (event.type === 'RoomJoined') joined = event.data.room_id;
        },
      );
    } else {
      await store.run({ type: 'LeaveRoom', data: { account, room_id: roomId, forget: false } });
    }
  } catch (error) {
    // A cancelled request says nothing: the user did it.
    if (error instanceof CommandFailed && error.cancelled) return { kind: 'failed', error: null };

    return { kind: 'failed', error: error instanceof Error ? error.message : String(error) };
  }

  // The next room list drops it too; until then it shouldn't be offered again.
  store.update((s) =>
    updateAccount(s, account, (a) => ({
      ...a,
      invitations: a.invitations.filter((i) => i.room_id !== roomId),
    })),
  );

  return accept ? { kind: 'joined', roomId: joined } : { kind: 'declined' };
}

/** What the invitation sheet does once its answer ended. */
export type AfterAnswer =
  | { type: 'stay' }
  | { type: 'close' }
  | { type: 'room'; roomId: string }
  | { type: 'space'; spaceId: string };

/**
 * Where the sheet goes once `result` came: a failure stays to be tried again, a decline closes,
 * a join opens the room (a space becomes the scope). Once the user left the sheet (`shown`
 * false) nothing moves: the answer still counts, but the screen they went to stays.
 */
export function afterAnswer(result: InvitationAnswer, space: boolean, shown: boolean): AfterAnswer {
  if (!shown || result.kind === 'failed') return { type: 'stay' };

  if (result.kind === 'declined') return { type: 'close' };

  return space
    ? { type: 'space', spaceId: result.roomId }
    : { type: 'room', roomId: result.roomId };
}
