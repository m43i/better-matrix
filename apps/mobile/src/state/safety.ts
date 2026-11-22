// What the user keeps away on each account (ADR 0011): the engine's `Safety` choices, decided
// here as `matrix_core::Safety` decides them (`shows_media`, `shows_invitation`, `ignores`), so
// both apps hide the same things.

import type {
  AccountId,
  Invitation,
  Safety,
  SafetyChange,
} from '@better-matrix/matrix-native/types';

/** Before the engine reports an account's choices: everything shows, as with none saved. */
export const DEFAULT_SAFETY: Safety = {
  ignored: [],
  media: 'Always',
  invite_pictures: true,
  hide_stranger_invites: false,
};

/** Whether a picture or video in a room shows without a tap. The user's own always do. */
export function showsMedia(safety: Safety, publicRoom: boolean, fromSelf: boolean): boolean {
  if (fromSelf) return true;

  switch (safety.media) {
    case 'Always':
      return true;
    case 'Private':
      return !publicRoom;
    case 'Never':
      return false;
  }
}

/** Whether an invitation is listed (in Home's section and its counter). */
export function showsInvitation(safety: Safety, invitation: Invitation): boolean {
  return !(safety.hide_stranger_invites && invitation.stranger);
}

/** Whether the user ignores someone. */
export function ignores(safety: Safety, userId: string): boolean {
  return safety.ignored.includes(userId);
}

/** The choices after `change`, as the engine answers `SetSafety`. */
export function withChange(safety: Safety, change: SafetyChange): Safety {
  switch (change.type) {
    case 'Ignore': {
      const others = safety.ignored.filter((id) => id !== change.data.user_id);
      const ignored = change.data.ignored ? [...others, change.data.user_id].sort() : others;

      return { ...safety, ignored };
    }

    case 'Media':
      return { ...safety, media: change.data };
    case 'InvitePictures':
      return { ...safety, invite_pictures: change.data };
    case 'HideStrangerInvites':
      return { ...safety, hide_stranger_invites: change.data };
  }
}

/** What `shownInvitations` answered per list while strangers are hidden: one array per list. */
const shownCache = new WeakMap<Invitation[], Invitation[]>();

/**
 * The invitations listed: all of them unless strangers' are hidden. The same list and choices
 * give the same array, so views comparing with `shallow` don't redraw.
 */
export function shownInvitations(invitations: Invitation[], safety: Safety): Invitation[] {
  if (!safety.hide_stranger_invites || !invitations.some((i) => i.stranger)) return invitations;
  const cached = shownCache.get(invitations);

  if (cached) return cached;
  const shown = invitations.filter((invitation) => showsInvitation(safety, invitation));
  shownCache.set(invitations, shown);

  return shown;
}

/** The invitations kept out of sight, which Settings → Safety lists. */
export function hiddenInvitations(
  invitations: readonly Invitation[],
  safety: Safety,
): Invitation[] {
  return invitations.filter((invitation) => !showsInvitation(safety, invitation));
}

/** A message's key in `State.revealedMedia`: media the user tapped Show on, until restart. */
export function revealKey(account: AccountId, eventId: string): string {
  return `${account}|${eventId}`;
}
