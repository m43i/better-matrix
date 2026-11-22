// People and rooms, as plain data: finding members, what the user may do to them, searching the
// user directory as it is typed, and renaming rooms (making them is state/admin.ts). The
// desktop's rules (apps/desktop/src/state/search.rs, matrix-core's `Members::may`), shared by the
// screens in ui/people and tested in __tests__/people.test.ts.

import type {
  AccountId,
  AttachmentKind,
  Command,
  MemberRole,
  Members,
  Moderation,
  NotificationMode,
  Person,
  RequestId,
  RoomMember,
  RoomSummary,
  SpaceSummary,
} from '@better-matrix/matrix-native/types';

// ----- finding members ---------------------------------------------------------------------

/**
 * Whether `name` matches a typed query: every word of it starts a word of the name, ignoring
 * case ("gr ho" finds "Grace Hopper").
 */
export function matches(name: string, query: string): boolean {
  const words = name
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => w.length > 0);

  return query
    .toLowerCase()
    .split(/\s+/)
    .filter((part) => part.length > 0)
    .every((part) => words.some((word) => word.startsWith(part)));
}

/**
 * The members a search lets through, in the room's order (the engine sorts them: those around
 * first, then by role, then by name): by name, or by any part of their user ID.
 */
export function filterMembers(people: RoomMember[], query: string): RoomMember[] {
  const trimmed = query.trim();

  if (trimmed === '') return people;
  const id = trimmed.toLowerCase();

  return people.filter(
    (m) => matches(m.person.name, trimmed) || m.person.id.toLowerCase().includes(id),
  );
}

// ----- moderation ----------------------------------------------------------------------------

/** The power level a role gives, as the engine's `MemberRole::level`. */
export function roleLevel(role: MemberRole): number {
  switch (role) {
    case 'Admin':
      return 100;
    case 'Moderator':
      return 50;
    case 'Member':
      return 0;
  }
}

/** The roles from the highest down. */
export const ROLES: readonly MemberRole[] = ['Admin', 'Moderator', 'Member'];

/**
 * Whether the user may do `action` to `member`, as the engine's `Members::may` decides: with
 * the right, and only to members below the user's own level; a role or level no higher than
 * their own. Approving or denying a join request and unbanning act on people outside the room.
 */
export function may(members: Members, action: Moderation, member: RoomMember): boolean {
  let allowed: boolean;

  switch (action.type) {
    case 'Kick':
      allowed = members.can_kick;
      break;
    case 'Ban':
      allowed = members.can_ban;
      break;
    case 'Role':
      allowed =
        members.can_set_roles &&
        roleLevel(action.data) <= members.own_level &&
        action.data !== member.role;
      break;
    case 'Level':
      allowed =
        members.can_set_roles && action.data <= members.own_level && action.data !== member.level;
      break;
    case 'Admit':
      // An invitation needs no rank above the one invited.
      return members.can_invite;
    case 'Deny':
      allowed = members.can_kick;
      break;
    case 'Unban':
      // Lifting a ban is a kick from it: both rights (Matrix's authorization rules).
      allowed = members.can_ban && members.can_kick;
      break;
  }

  return allowed && member.level < members.own_level;
}

/** The roles the user may give `member`. */
export function rolesOffered(members: Members, member: RoomMember): MemberRole[] {
  return ROLES.filter((role) => may(members, { type: 'Role', data: role }, member));
}

/** Whether the user may give `member` a level of their own choosing (any up to their own). */
export function mayChooseLevel(members: Members, member: RoomMember): boolean {
  return members.can_set_roles && member.level < members.own_level;
}

/**
 * Whether `action` waits for the user's confirmation: kicking, banning and denying a request
 * always, a role or level only when it reaches the user's own (equals can't demote each other, so
 * it can't be undone); approving a request and unbanning never.
 */
export function asksFirst(action: Moderation, ownLevel: number): boolean {
  switch (action.type) {
    case 'Role':
      return roleLevel(action.data) >= ownLevel;
    case 'Level':
      return action.data >= ownLevel;
    case 'Admit':
    case 'Unban':
      return false;
    case 'Kick':
    case 'Ban':
    case 'Deny':
      return true;
  }
}

/** A member of a room, by user ID. */
export function memberOf(members: Members | undefined, userId: string): RoomMember | undefined {
  return members?.people.find((m) => m.person.id === userId);
}

// ----- rooms ---------------------------------------------------------------------------------

/** One kind of command. */
export type CommandOf<T extends Command['type']> = Extract<Command, { type: T }>;

/** The account's group rooms to invite someone to, besides `current`. */
export function inviteTargets(rooms: RoomSummary[], current: string): RoomSummary[] {
  return rooms.filter((r) => r.kind === 'Group' && r.room_id !== current);
}

/** The spaces a new room may go in: those the user may add rooms to. */
export function spacesForNewRoom(spaces: SpaceSummary[]): SpaceSummary[] {
  return spaces.filter((s) => s.can_add_rooms);
}

/** `room` as its notification mode leaves it: its own, or with `null` the account's default. */
export function withNotificationMode(
  room: RoomSummary,
  mode: NotificationMode | null,
  fallback: NotificationMode,
): RoomSummary {
  return { ...room, notifications: mode ?? fallback, custom_notifications: mode !== null };
}

/** How a shared file opens: images and videos in the preview, anything else is shared. */
export function fileOpens(kind: AttachmentKind): 'preview' | 'share' {
  return kind === 'Image' || kind === 'Video' ? 'preview' : 'share';
}

// ----- searching the user directory ---------------------------------------------------------

/** How long the text must rest before it is searched: a word typed letter by letter is one search. */
export const SEARCH_PAUSE_MS = 300;

/** A search of the user directory as it is typed. Only the answer to the latest search counts. */
export interface PeopleSearch {
  /** The text as typed. */
  text: string;
  /** The text the results are for: `null` while the text has moved on and isn't searched yet. */
  searched: string | null;
  /** The search on its way, if any. */
  request: RequestId | null;
  /** Who the latest search found. */
  people: Person[];
  /** Whom to suggest before anything is typed (the people the user shares rooms with). */
  suggested: Person[];
  /** The suggestion request on its way, if any. */
  suggesting: RequestId | null;
}

export const EMPTY_SEARCH: PeopleSearch = {
  text: '',
  searched: '',
  request: null,
  people: [],
  suggested: [],
  suggesting: null,
};

/** The trimmed query. */
export function query(search: PeopleSearch): string {
  return search.text.trim();
}

/**
 * The search after the text changed. A new query drops the answer to the last one (it no longer
 * fits) until its own search goes out; clearing the text clears the results at once.
 */
export function typed(search: PeopleSearch, text: string): PeopleSearch {
  const next = text.trim();

  if (next === query(search)) return { ...search, text };

  if (next === '') return { ...search, text, searched: '', request: null, people: [] };

  return { ...search, text, searched: null, request: null };
}

/** The text to search for now, if it changed since the last search. */
export function due(search: PeopleSearch): string | null {
  const text = query(search);

  return text !== '' && search.searched !== text ? text : null;
}

/** The search after `request` went out for `term`. */
export function searching(search: PeopleSearch, term: string, request: RequestId): PeopleSearch {
  // The text may have moved on while the search was resting; only the current one goes out.
  if (term !== query(search)) return search;

  return { ...search, searched: term, request };
}

/** The search after `request` found `people`; an answer to an older search is dropped. */
export function found(search: PeopleSearch, request: RequestId, people: Person[]): PeopleSearch {
  if (search.suggesting === request) return { ...search, suggested: people, suggesting: null };

  if (search.request !== request) return search;

  return { ...search, people };
}

/** The search after `request` finished: nothing is on its way for it any more. */
export function finished(search: PeopleSearch, request: RequestId): PeopleSearch {
  if (search.request === request) return { ...search, request: null };

  if (search.suggesting === request) return { ...search, suggesting: null };

  return search;
}

/** Whether `text` is a full Matrix user ID, `@name:server`. */
export function isUserId(text: string): boolean {
  return /^@[^\s:@]+:[^\s:]+(:\d+)?$/.test(text.trim());
}

/**
 * Whom to list: the suggestions before anything is typed, else the people last found. A typed full
 * user ID is offered first even when the directory doesn't know it. The user is never listed.
 */
export function candidates(search: PeopleSearch, me: AccountId): Person[] {
  const text = query(search);
  // While new text rests, the last results stay: the list changes once, when the answer comes.
  const list = text === '' ? search.suggested : search.people;
  const people = list.filter((p) => p.id !== me);

  if (
    isUserId(text) &&
    text !== me &&
    !people.some((p) => p.id.toLowerCase() === text.toLowerCase())
  ) {
    return [{ id: text, name: text, avatar: null }, ...people];
  }

  return people;
}

/** Whether a search is on its way for the text as typed. */
export function busy(search: PeopleSearch): boolean {
  const text = query(search);

  if (text === '') return search.suggesting !== null;

  return search.searched !== text || search.request !== null;
}

// ----- dates ---------------------------------------------------------------------------------

/** One calendar day on this device: its year, month (1–12) and day. */
export interface Day {
  year: number;
  month: number;
  day: number;
}

/** Which day `ms` falls on, relative to `now`: today, yesterday, or a date. */
export function dayOf(ms: number, now: number): 'today' | 'yesterday' | Day {
  const at = new Date(ms);
  const today = new Date(now);

  const same = (a: Date, b: Date) =>
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate();

  if (same(at, today)) return 'today';
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);

  if (same(at, yesterday)) return 'yesterday';

  return { year: at.getFullYear(), month: at.getMonth() + 1, day: at.getDate() };
}
