// The room list's sections for one scope, as the desktop's sidebar arranges them
// (apps/desktop/src/state/sections.rs and accounts.rs): the built-in Rooms and Direct messages,
// which always exist and list the latest conversation first, and the user's own sections, which
// hold the rooms and chats moved into them in the order they were placed. Also which rooms a
// scope shows, which "want attention", and the edits to a `SidebarLayout` the user can make.
// Pure: every function takes what it needs; layout edits return new values, and what the scope
// helpers derive is kept per list and must not be changed.

import type {
  Message,
  RoomKind,
  RoomSummary,
  ScopeLayout,
  SidebarLayout,
  SpaceSummary,
} from '@better-matrix/matrix-native/types';

import { roomKey, timelineId } from './timeline';
import type { AccountData, Scope } from './types';

/** The built-in sections' IDs: they can be collapsed, never removed. */
export const ROOMS = 'rooms';

export const DIRECT = 'direct';

/** Whether a section is one of the built-in ones. */
export function isBuiltin(id: string): boolean {
  return id === ROOMS || id === DIRECT;
}

/** Where a scope's layout is kept: `home`, or the space's room ID. */
export function scopeKey(scope: Scope): string {
  return scope === 'home' ? 'home' : scope.space;
}

export function sameScope(a: Scope, b: Scope): boolean {
  return scopeKey(a) === scopeKey(b);
}

/**
 * Whether a room has something the user wants to see: unread messages when it notifies for all
 * of them, else mentions.
 */
export function wantsAttention(room: RoomSummary): boolean {
  return room.notifications === 'AllMessages' ? room.unread > 0 : room.mentions > 0;
}

// What the room lists derive from the spaces and rooms is kept per array: state is never changed
// in place, so it stays true while the reducer keeps the array, and the selectors asking on every
// store change (the dots of the space picker, the tab header and the account menu) find it ready.

/** The spaces by ID and what was found under each, per spaces list. */
interface SpaceIndex {
  byId: Map<string, SpaceSummary>;
  /** Each space's rooms, as `spaceRooms` found them. */
  rooms: Map<string, ReadonlySet<string>>;
  /** Every room in some space, once Home asked. */
  inSpaces: ReadonlySet<string> | null;
}

const spaceIndexes = new WeakMap<readonly SpaceSummary[], SpaceIndex>();

function spaceIndex(spaces: readonly SpaceSummary[]): SpaceIndex {
  let index = spaceIndexes.get(spaces);

  if (!index) {
    index = { byId: new Map(spaces.map((s) => [s.room_id, s])), rooms: new Map(), inSpaces: null };
    spaceIndexes.set(spaces, index);
  }

  return index;
}

/** Every room ID under a space, following subspaces; safe against cycles. */
export function spaceRooms(spaces: readonly SpaceSummary[], spaceId: string): ReadonlySet<string> {
  const index = spaceIndex(spaces);
  const known = index.rooms.get(spaceId);

  if (known) return known;
  const rooms = new Set<string>();
  const seen = new Set<string>();
  const pending = [spaceId];

  for (let id = pending.pop(); id !== undefined; id = pending.pop()) {
    if (seen.has(id)) continue;
    seen.add(id);
    const space = index.byId.get(id);

    if (!space) continue;

    for (const child of space.children) {
      if (index.byId.has(child)) pending.push(child);
      else rooms.add(child);
    }
  }

  index.rooms.set(spaceId, rooms);

  return rooms;
}

/** Every room in some space: what Home leaves out of its group rooms. */
function roomsInSpaces(spaces: readonly SpaceSummary[]): ReadonlySet<string> {
  const index = spaceIndex(spaces);

  if (!index.inSpaces) {
    const inSpaces = new Set<string>();

    for (const space of spaces)
      for (const id of spaceRooms(spaces, space.room_id)) inSpaces.add(id);
    index.inSpaces = inSpaces;
  }

  return index.inSpaces;
}

/** What was worked out for one rooms and spaces list, by scope (and kind). */
interface ScopeIndex {
  rooms: Map<string, readonly RoomSummary[]>;
  unread: Map<string, boolean>;
}

const scopeIndexes = new WeakMap<
  readonly RoomSummary[],
  WeakMap<readonly SpaceSummary[], ScopeIndex>
>();

function scopeIndex(account: Pick<AccountData, 'rooms' | 'spaces'>): ScopeIndex {
  let bySpaces = scopeIndexes.get(account.rooms);

  if (!bySpaces) {
    bySpaces = new WeakMap();
    scopeIndexes.set(account.rooms, bySpaces);
  }

  let index = bySpaces.get(account.spaces);

  if (!index) {
    index = { rooms: new Map(), unread: new Map() };
    bySpaces.set(account.spaces, index);
  }

  return index;
}

function findRoomsInScope(
  account: Pick<AccountData, 'rooms' | 'spaces'>,
  scope: Scope,
  kind: RoomKind | undefined,
): RoomSummary[] {
  const ofKind = kind ? account.rooms.filter((r) => r.kind === kind) : account.rooms;

  if (scope === 'home') {
    const inSpaces = roomsInSpaces(account.spaces);

    return ofKind.filter((room) => room.kind === 'Direct' || !inSpaces.has(room.room_id));
  }

  const rooms = spaceRooms(account.spaces, scope.space);
  const members = new Set(account.spaces.find((s) => s.room_id === scope.space)?.members ?? []);

  return ofKind.filter(
    (room) =>
      rooms.has(room.room_id) ||
      (room.kind === 'Direct' && room.dm_users.some((u) => members.has(u))),
  );
}

/**
 * The rooms a scope shows. Home: every direct chat, and the group rooms in no space. A space:
 * its rooms (subspaces included), and direct chats in it or with its members. The same list
 * while the account's rooms and spaces are.
 */
export function roomsInScope(
  account: Pick<AccountData, 'rooms' | 'spaces'>,
  scope: Scope,
  kind?: RoomKind,
): readonly RoomSummary[] {
  const index = scopeIndex(account);
  const key = kind === undefined ? scopeKey(scope) : `${kind} ${scopeKey(scope)}`;
  let rooms = index.rooms.get(key);

  if (!rooms) {
    rooms = findRoomsInScope(account, scope, kind);
    index.rooms.set(key, rooms);
  }

  return rooms;
}

/** Whether any room in the scope wants attention, for the dots in the space picker and account menu. */
export function hasUnread(account: Pick<AccountData, 'rooms' | 'spaces'>, scope: Scope): boolean {
  const index = scopeIndex(account);
  const key = scopeKey(scope);
  let unread = index.unread.get(key);

  if (unread === undefined) {
    unread = roomsInScope(account, scope).some(wantsAttention);
    index.unread.set(key, unread);
  }

  return unread;
}

/** The newest message in a room: from its timeline when open, else from sync. */
export function lastMessage(
  account: Pick<AccountData, 'timelines' | 'latest'>,
  roomId: string,
): Message | undefined {
  const entries = account.timelines[timelineId(roomKey(roomId))]?.entries ?? [];
  let fromTimeline: Message | undefined;

  for (let i = entries.length - 1; i >= 0; i--) {
    const kind = entries[i]?.kind;

    if (kind?.type === 'Message') {
      fromTimeline = kind.data;
      break;
    }
  }

  const synced = account.latest[roomId];

  if (fromTimeline && synced)
    return fromTimeline.timestamp_ms >= synced.timestamp_ms ? fromTimeline : synced;

  return fromTimeline ?? synced;
}

/** How far ahead of this device's clock a server's may be. */
const DRIFT_MS = 5 * 60_000;

/**
 * When something last happened in a room, for the list's order: its newest message seen, or the
 * latest event the store kept from before; 0 when nothing is known. A time further in the future
 * than clocks drift doesn't count, so a misdated event can't hold a room at the top: a message
 * heard this session counts from when it was heard instead.
 */
export function activity(
  account: Pick<AccountData, 'timelines' | 'latest' | 'heardAt'>,
  room: RoomSummary,
  now: number,
): number {
  const plausible = (at: number) => at <= now + DRIFT_MS;
  const message = lastMessage(account, room.room_id);

  const seen = message
    ? plausible(message.timestamp_ms)
      ? message.timestamp_ms
      : account.heardAt[room.room_id]
    : undefined;

  const stored =
    room.activity_ms !== null && plausible(room.activity_ms) ? room.activity_ms : undefined;

  return Math.max(seen ?? 0, stored ?? 0);
}

/** Rooms latest conversation first; rooms with nothing known yet after them, by name. */
export function byRecency(
  account: Pick<AccountData, 'timelines' | 'latest' | 'heardAt'>,
  rooms: readonly RoomSummary[],
  now: number,
): RoomSummary[] {
  const keyed = rooms.map((room) => ({
    room,
    at: activity(account, room, now),
    name: room.name.toLowerCase(),
  }));

  keyed.sort((a, b) => b.at - a.at || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

  return keyed.map((k) => k.room);
}

/** One section as the room list shows it. */
export interface SectionView {
  id: string;
  /** The user's name for it; `null` for a built-in section, which the view names. */
  name: string | null;
  collapsed: boolean;
  rooms: RoomSummary[];
}

function scopeLayout(layout: SidebarLayout, scope: Scope): ScopeLayout | undefined {
  return layout.scopes[scopeKey(scope)];
}

/**
 * A scope's sections in order: the user's own, then Rooms and Direct messages with whatever isn't
 * in one of the user's; sections the saved order names come first, in its order.
 */
export function sections(
  account: Pick<AccountData, 'sidebar' | 'rooms' | 'spaces' | 'timelines' | 'latest' | 'heardAt'>,
  scope: Scope,
  now: number,
): SectionView[] {
  const kept = scopeLayout(account.sidebar, scope);
  const collapsed = new Set(kept?.collapsed ?? []);
  const inScope = roomsInScope(account, scope);
  const byId = new Map(inScope.map((r) => [r.room_id, r]));
  const placed = new Set<string>();
  const out: SectionView[] = [];

  for (const section of kept?.sections ?? []) {
    const rooms = section.rooms.flatMap((id) => byId.get(id) ?? []);

    for (const room of rooms) placed.add(room.room_id);
    out.push({ id: section.id, name: section.name, collapsed: collapsed.has(section.id), rooms });
  }

  for (const [id, kind] of [
    [ROOMS, 'Group'],
    [DIRECT, 'Direct'],
  ] as const) {
    const rooms = inScope.filter((room) => room.kind === kind && !placed.has(room.room_id));
    out.push({
      id,
      name: null,
      collapsed: collapsed.has(id),
      rooms: byRecency(account, rooms, now),
    });
  }

  const order = kept?.order ?? [];

  const rank = (id: string) => {
    const at = order.indexOf(id);

    return at < 0 ? Number.MAX_SAFE_INTEGER : at;
  };

  // A stable sort: sections the order doesn't name keep their place after the ones it does.
  return out
    .map((view, index) => ({ view, index }))
    .sort((a, b) => rank(a.view.id) - rank(b.view.id) || a.index - b.index)
    .map((x) => x.view);
}

/** The rooms of a scope wanting attention: mentions first, then by unread count. */
export function unreadRooms(
  account: Pick<AccountData, 'rooms' | 'spaces'>,
  scope: Scope,
): RoomSummary[] {
  return roomsInScope(account, scope)
    .filter(wantsAttention)
    .sort((a, b) => b.mentions - a.mentions || b.unread - a.unread);
}

/** Every direct chat of an account, latest first. */
export function directChats(
  account: Pick<AccountData, 'rooms' | 'timelines' | 'latest' | 'heardAt'>,
  now: number,
): RoomSummary[] {
  return byRecency(
    account,
    account.rooms.filter((r) => r.kind === 'Direct'),
    now,
  );
}

/** The spaces in the user's order: those the layout names, in its order, then the rest. */
export function orderedSpaces(
  spaces: readonly SpaceSummary[],
  layout: SidebarLayout,
): SpaceSummary[] {
  const order = layout.spaces ?? [];

  const rank = (id: string) => {
    const at = order.indexOf(id);

    return at < 0 ? Number.MAX_SAFE_INTEGER : at;
  };

  return spaces
    .map((space, index) => ({ space, index }))
    .sort((a, b) => rank(a.space.room_id) - rank(b.space.room_id) || a.index - b.index)
    .map((x) => x.space);
}

/** The user's section in `scope` holding `roomId`, if any. */
export function sectionOf(layout: SidebarLayout, scope: Scope, roomId: string): string | null {
  return scopeLayout(layout, scope)?.sections.find((s) => s.rooms.includes(roomId))?.id ?? null;
}

/** The user's own sections of a scope, in the order they show. */
export function ownSections(account: AccountData, scope: Scope, now: number): SectionView[] {
  return sections(account, scope, now).filter((s) => !isBuiltin(s.id));
}

// ----- layout edits: each returns a new layout --------------------------------------------------

const EMPTY_SCOPE: ScopeLayout = { collapsed: [], sections: [], order: [] };

function withScope(
  layout: SidebarLayout,
  scope: Scope,
  change: (s: ScopeLayout) => ScopeLayout,
): SidebarLayout {
  const key = scopeKey(scope);

  return {
    ...layout,
    scopes: { ...layout.scopes, [key]: change(layout.scopes[key] ?? EMPTY_SCOPE) },
  };
}

/** Collapses a section, or expands it again. */
export function toggleCollapsed(layout: SidebarLayout, scope: Scope, id: string): SidebarLayout {
  return withScope(layout, scope, (s) => ({
    ...s,
    collapsed: s.collapsed.includes(id)
      ? s.collapsed.filter((c) => c !== id)
      : [...s.collapsed, id],
  }));
}

/** Adds a section of the user's, first in the scope. Returns the layout and the section's ID. */
export function addSection(layout: SidebarLayout, scope: Scope, name: string) {
  const taken = new Set((scopeLayout(layout, scope)?.sections ?? []).map((s) => s.id));
  let n = 1;

  while (taken.has(`section-${n}`)) n++;
  const id = `section-${n}`;

  const next = withScope(layout, scope, (s) => ({
    ...s,
    sections: [{ id, name: name.trim(), rooms: [] }, ...s.sections],
    order: s.order.length > 0 ? [id, ...s.order] : s.order,
  }));

  return { layout: next, id };
}

export function renameSection(
  layout: SidebarLayout,
  scope: Scope,
  id: string,
  name: string,
): SidebarLayout {
  return withScope(layout, scope, (s) => ({
    ...s,
    sections: s.sections.map((section) =>
      section.id === id ? { ...section, name: name.trim() } : section,
    ),
  }));
}

/** Removes a section of the user's; its rooms go back to the built-in sections. */
export function deleteSection(layout: SidebarLayout, scope: Scope, id: string): SidebarLayout {
  return withScope(layout, scope, (s) => ({
    sections: s.sections.filter((section) => section.id !== id),
    collapsed: s.collapsed.filter((c) => c !== id),
    order: s.order.filter((o) => o !== id),
  }));
}

/**
 * Moves a room into a section at `index` among its rooms (the end when omitted): one of the
 * user's, or a built-in one, which takes it out of the user's sections.
 */
export function moveRoom(
  layout: SidebarLayout,
  scope: Scope,
  roomId: string,
  to: string,
  index?: number,
): SidebarLayout {
  return withScope(layout, scope, (s) => ({
    ...s,
    sections: s.sections.map((section) => {
      const rooms = section.rooms.filter((r) => r !== roomId);

      if (section.id === to) rooms.splice(Math.min(index ?? rooms.length, rooms.length), 0, roomId);

      return { ...section, rooms };
    }),
  }));
}

/** Moves a section to `index` among the sections `shown`, in their current order. */
export function moveSection(
  layout: SidebarLayout,
  scope: Scope,
  shown: readonly string[],
  id: string,
  index: number,
): SidebarLayout {
  const order = shown.filter((known) => known !== id);
  order.splice(Math.min(index, order.length), 0, id);

  return withScope(layout, scope, (s) => ({ ...s, order }));
}

/** Moves a space to `index` among the spaces `shown`, in their order. */
export function moveSpace(
  layout: SidebarLayout,
  shown: readonly string[],
  id: string,
  index: number,
): SidebarLayout {
  const spaces = shown.filter((known) => known !== id);
  spaces.splice(Math.min(index, spaces.length), 0, id);

  return { ...layout, spaces };
}

/** The layout marked as changed now: later than the one it changes, even with a clock behind. */
export function stamped(
  layout: SidebarLayout,
  previous: SidebarLayout,
  now: number,
): SidebarLayout {
  return { ...layout, changed_at: Math.max(now, previous.changed_at + 1) };
}
