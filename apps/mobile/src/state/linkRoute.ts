// Where a permalink goes, decided from what this device knows: a room one of the accounts has
// joined opens at once (at the linked message); anything else opens the link screen, which looks
// the room up (or the person) for one account and offers to join (or chat).

import type { AccountId, Person, RoomPreview } from '@better-matrix/matrix-native/types';

import type { Key } from '../i18n';

import type { Permalink } from './permalinks';
import type { State } from './types';

/** Where a link goes at once. */
export type LinkRoute =
  /** A room an account has joined, by its ID: open it, at the message when the link names one. */
  | { type: 'room'; account: AccountId; roomId: string; eventId: string | null }
  /** A space an account has joined, by its ID: show it, as the account's scope. */
  | { type: 'space'; account: AccountId; spaceId: string }
  /** The link screen, acting for `account` (`null` while no account is signed in). */
  | { type: 'screen'; account: AccountId | null }
  /** Not a link to anything in Matrix. */
  | { type: 'invalid' };

/**
 * The accounts that have joined the room or space `roomId`, `preferred` first, then in the order
 * they were added.
 */
export function joinedAccounts(
  state: State,
  roomId: string,
  preferred: AccountId | null = null,
): AccountId[] {
  const has = (a: State['accounts'][number]) =>
    a.rooms.some((r) => r.room_id === roomId) || a.spaces.some((s) => s.room_id === roomId);

  const joined = state.accounts.filter(has).map((a) => a.summary.id);

  return preferred !== null && joined.includes(preferred)
    ? [preferred, ...joined.filter((id) => id !== preferred)]
    : joined;
}

/** The account a link acts for: `preferred` (the account it was tapped in) when signed in, else the one on screen. */
export function linkAccount(state: State, preferred: AccountId | null = null): AccountId | null {
  const signedIn = (id: AccountId | null) =>
    id !== null && state.accounts.some((a) => a.summary.id === id);

  if (signedIn(preferred)) return preferred;

  if (signedIn(state.active)) return state.active;

  return state.accounts[0]?.summary.id ?? null;
}

/** Where `link` goes, preferring `preferred` (the account it was tapped in). */
export function routeLink(
  state: State,
  link: Permalink | null,
  preferred: AccountId | null = null,
): LinkRoute {
  if (link === null) return { type: 'invalid' };
  const account = linkAccount(state, preferred);

  // An alias can't be checked here: the link screen learns its room ID from the preview.
  if (link.kind === 'room' && link.id.startsWith('!')) {
    const joined = joinedAccounts(state, link.id, account)[0];

    if (joined !== undefined) return joinedRoute(state, joined, link.id, false, link.event);
  }

  return { type: 'screen', account };
}

/**
 * Where a room or space `account` has joined opens: a space (known as one, or `space` by its
 * preview while the room list doesn't say yet) as the account's scope, a room at `eventId`.
 */
export function joinedRoute(
  state: State,
  account: AccountId,
  roomId: string,
  space: boolean,
  eventId: string | null = null,
): Extract<LinkRoute, { type: 'room' | 'space' }> {
  const known =
    state.accounts
      .find((a) => a.summary.id === account)
      ?.spaces.some((s) => s.room_id === roomId) ?? false;

  return space || known
    ? { type: 'space', account, spaceId: roomId }
    : { type: 'room', account, roomId, eventId };
}

/**
 * The account to open a previewed room in, if one has joined it: the one the preview was for,
 * else (unless the user picked the account themselves) any other.
 */
export function joinedAfterPreview(
  state: State,
  preview: RoomPreview,
  account: AccountId,
  chosen: boolean,
): AccountId | null {
  if (preview.joined) return account;
  const joined = joinedAccounts(state, preview.room_id, account);

  if (chosen) return joined.includes(account) ? account : null;

  return joined[0] ?? null;
}

/** Whether Join may work: invited, or a rule that lets the user in (asking to join isn't offered). */
export function joinable(preview: RoomPreview): boolean {
  return (
    preview.invited ||
    preview.join_rule === null ||
    preview.join_rule === 'Public' ||
    preview.join_rule === 'Restricted'
  );
}

/** What the link screen shows for a room before Join: its preview, or none from any server. */
export type Previewed =
  | { type: 'preview'; preview: RoomPreview }
  | { type: 'failed'; error: string };

/** Where the link screen stands. */
export type LinkStep =
  /** Looking the room up. */
  | { type: 'loading' }
  | Previewed
  /** Joining, from what was shown before (where a failure goes back to). */
  | { type: 'joining'; before: Previewed }
  /** A person, while their profile loads and once it did (`null` when their server didn't say). */
  | { type: 'person'; person: Person | null; loading: boolean; chatting?: boolean };

/** What the link screen says under a room, in the desktop's words, when something needs saying. */
export function linkNote(step: LinkStep): Key | null {
  if (step.type === 'failed') return 'link.no_preview';

  if (step.type !== 'preview') return null;
  const { preview } = step;

  if (preview.invited) return 'link.invited';

  if (joinable(preview)) return null;

  return preview.join_rule === 'Knock' ? 'link.knock' : 'link.invite_only';
}

/** The preview a step shows: its own, or the one a join started from. */
export function shownPreview(step: LinkStep): RoomPreview | null {
  if (step.type === 'preview') return step.preview;

  if (step.type === 'joining' && step.before.type === 'preview') return step.before.preview;

  return null;
}

/**
 * Whether another account may be picked: not while a join or a chat is under way, which acts
 * for the account it started with (the desktop's `choose_link_account`).
 */
export function mayChooseAccount(step: LinkStep): boolean {
  return step.type !== 'joining' && !(step.type === 'person' && step.chatting);
}

/** Whether the step offers Join: a joinable preview, no preview at all, or a join under way. */
export function offersJoin(step: LinkStep): boolean {
  if (step.type === 'failed') return true;

  if (step.type === 'preview') return joinable(step.preview);

  return step.type === 'joining';
}
