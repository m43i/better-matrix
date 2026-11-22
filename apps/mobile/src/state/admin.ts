// Room and space administration, as plain data: the new-room form's choices and command, which
// parts of a room's settings the user may change (`RoomRights`), permission levels, a space's
// rooms in order, leaving a space with some of its rooms, and an upgraded room's successor
// (ADR 0008: Making rooms, Room settings, Room upgrades, Space rooms). Tested in
// __tests__/admin.test.ts.

import type {
  AccountId,
  JoinRule,
  Permission,
  PermissionLevel,
  RoomSetting,
  RoomSettings,
  RoomSummary,
  SpaceChange,
  SpaceSummary,
  Successor,
} from '@better-matrix/matrix-native/types';

import type { CommandOf } from './people';

// ----- making rooms and spaces ---------------------------------------------------------------

/**
 * The rules a new room or space may start with, in the form's order. A space is private or
 * public: nowhere would answer requests to join it.
 */
export function joinRulesOffered(space: boolean, inSpace: boolean): JoinRule[] {
  if (space) return ['Invite', 'Public'];

  return inSpace ? ['Invite', 'Restricted', 'Knock', 'Public'] : ['Invite', 'Knock', 'Public'];
}

/** The server a user ID lives on (`example.org` for `@ada:example.org`): where addresses go. */
export function serverOf(userId: string): string {
  return userId.slice(userId.indexOf(':') + 1);
}

/** A join rule from a route's params, if it names one. */
export function joinRuleParam(param: string | undefined): JoinRule | null {
  return param === 'Public' || param === 'Knock' || param === 'Invite' || param === 'Restricted'
    ? param
    : null;
}

/** What the new-room and new-space forms hold. */
export interface NewRoomDraft {
  name: string;
  topic: string;
  /** The space a room goes in, if any. */
  parent: string | null;
  joinRule: JoinRule;
  /** The main address's local part, as typed (only for a public one). */
  address: string;
  /** Listed in the server's directory (only for a public one). */
  published: boolean;
  /** Encryption as the user set it; `null` until they touched it, so the rule decides. */
  encrypted: boolean | null;
}

export const EMPTY_NEW_ROOM: NewRoomDraft = {
  name: '',
  topic: '',
  parent: null,
  joinRule: 'Invite',
  address: '',
  published: false,
  encrypted: null,
};

/** Whether a new room is encrypted: as the user set it, else unless anyone may join. */
export function encryptsByDefault(draft: NewRoomDraft): boolean {
  return draft.encrypted ?? draft.joinRule !== 'Public';
}

/**
 * The local part of an address as typed: without a leading `#` or a `:server`, trimmed. `null`
 * when nothing is left or it holds what an alias can't (white space, `:`).
 */
export function addressLocalPart(text: string): string | null {
  const local = text.trim().replace(/^#/, '').split(':')[0] ?? '';

  return local === '' || /[\s#:,]/.test(local) ? null : local;
}

/** Whether the address field holds something that can't be an address. */
export function addressInvalid(text: string): boolean {
  return text.trim() !== '' && addressLocalPart(text) === null;
}

/**
 * The command that makes the room or space the form describes, or `null` while it has no name or
 * its address can't be one. A space never goes in a space here and is never encrypted; Space
 * members needs the parent.
 */
export function createRoomCommand(
  account: AccountId,
  draft: NewRoomDraft,
  space: boolean,
): CommandOf<'CreateRoom'> | null {
  const name = draft.name.trim();

  const topic = draft.topic.trim();
  const parent = space ? null : draft.parent;
  // Only a rule the form offers (a link may name another): else private.
  const offered = joinRulesOffered(space, parent !== null);
  const joinRule = offered.includes(draft.joinRule) ? draft.joinRule : 'Invite';
  const isPublic = joinRule === 'Public';

  // The address counts only where it shows: for a public one.
  if (name === '' || (isPublic && addressInvalid(draft.address))) return null;

  return {
    type: 'CreateRoom',
    data: {
      account,
      room: {
        name,
        topic: topic === '' ? null : topic,
        space,
        parent,
        join_rule: joinRule,
        address: isPublic ? addressLocalPart(draft.address) : null,
        published: isPublic && draft.published,
        encrypted: !space && encryptsByDefault({ ...draft, joinRule }),
      },
    },
  };
}

// ----- room settings -------------------------------------------------------------------------

/** One part of the settings screen. */
export type SettingsPart =
  | 'general'
  | 'access'
  | 'addresses'
  | 'security'
  | 'permissions'
  | 'rooms'
  | 'advanced';

/**
 * The parts of the settings the user may change, in the screen's order: each only with its right
 * (read-only parts aren't shown). Encryption is offered while it is off; a space's rooms only for
 * a space.
 */
export function settingsParts(settings: RoomSettings): SettingsPart[] {
  const r = settings.rights;

  const parts: Array<[SettingsPart, boolean]> = [
    ['general', r.name || r.topic || r.avatar],
    ['access', r.join_rule || r.guest_access || r.history || directoryOffered(settings)],
    ['addresses', r.addresses],
    ['security', r.encryption && !settings.encrypted && !settings.space],
    ['permissions', r.permissions],
    ['rooms', r.space_rooms && settings.space],
    ['advanced', r.upgrade && !settings.space],
  ];

  return parts.flatMap(([part, shown]) => (shown ? [part] : []));
}

/** Whether the directory switch is offered: with the right, and when the server said. */
export function directoryOffered(settings: RoomSettings): boolean {
  return settings.rights.addresses && settings.published !== null;
}

/**
 * The rules the settings offer: Space members only where it would let someone in, Ask to join
 * only where the room can take requests (its version, and never a space); the current rule
 * always.
 */
export function joinRulesForSettings(settings: RoomSettings): JoinRule[] {
  const rules: JoinRule[] = ['Invite', 'Restricted', 'Knock', 'Public'];

  const offered: Record<JoinRule, boolean> = {
    Invite: true,
    Public: true,
    Restricted: settings.can_restrict,
    Knock: settings.can_knock,
  };

  return rules.filter((rule) => offered[rule] || settings.join_rule === rule);
}

/**
 * The settings as the screen shows them while changes are on their way: each pending change over
 * the engine's last snapshot, so a switch stays where the user put it until the room confirms.
 * A picture shows once the engine has it.
 */
export function withPending(settings: RoomSettings, pending: readonly RoomSetting[]): RoomSettings {
  let shown = settings;

  for (const change of pending) shown = withChange(shown, change);

  return shown;
}

/** The settings with one change made. */
function withChange(s: RoomSettings, change: RoomSetting): RoomSettings {
  switch (change.type) {
    case 'Name':
      return { ...s, name: change.data };
    case 'Topic':
      return { ...s, topic: change.data === '' ? null : change.data };
    case 'Avatar':
      return s;
    case 'JoinRule':
      return { ...s, join_rule: change.data };
    case 'History':
      return { ...s, history: change.data };
    case 'GuestAccess':
      return { ...s, guest_access: change.data };
    case 'Published':
      return { ...s, published: change.data };
    case 'AddAddress':
      return s.addresses.includes(change.data) || s.main_address === change.data
        ? s
        : {
            ...s,
            main_address: s.main_address ?? change.data,
            addresses: s.main_address === null ? s.addresses : [...s.addresses, change.data],
          };
    case 'RemoveAddress':
      return {
        ...s,
        main_address: s.main_address === change.data ? null : s.main_address,
        addresses: s.addresses.filter((a) => a !== change.data),
      };
    case 'MainAddress':
      return {
        ...s,
        main_address: change.data,
        addresses: [
          ...(s.main_address === null ? [] : [s.main_address]),
          ...s.addresses.filter((a) => a !== change.data && a !== s.main_address),
        ],
      };
    case 'Encrypt':
      return { ...s, encrypted: true };
    case 'Permission':
      return {
        ...s,
        permissions: s.permissions.map((p) =>
          p.permission === change.data.permission ? { ...p, level: change.data.level } : p,
        ),
      };
  }
}

/** The command that changes one setting. */
export function changeRoomCommand(
  account: AccountId,
  roomId: string,
  setting: RoomSetting,
): CommandOf<'ChangeRoom'> {
  return { type: 'ChangeRoom', data: { account, room_id: roomId, setting } };
}

/**
 * The text-field change for a name or topic, or `null` while it is unchanged (or a name is
 * blank): what Save sends.
 */
export function textSetting(
  which: 'Name' | 'Topic',
  current: string | null,
  typed: string,
): RoomSetting | null {
  const text = typed.trim();

  if (text === (current ?? '').trim()) return null;

  if (which === 'Name') return text === '' ? null : { type: 'Name', data: text };

  return { type: 'Topic', data: text };
}

/** The address to add, as the engine takes it (a local part is on the account's server). */
export function addressToAdd(settings: RoomSettings, typed: string): string | null {
  const text = typed.trim();

  if (text === '') return null;
  const full = text.includes(':') ? (text.startsWith('#') ? text : `#${text}`) : null;
  const local = addressLocalPart(text);

  if (local === null) return null;
  const alias = full ?? `#${local}:${settings.server}`;
  const known = settings.main_address === alias || settings.addresses.includes(alias);

  return known ? null : alias;
}

/** Every address, the main one first. */
export function allAddresses(settings: RoomSettings): string[] {
  const main = settings.main_address;
  const others = settings.addresses.filter((a) => a !== main);

  return main === null ? others : [main, ...others];
}

// ----- permissions ---------------------------------------------------------------------------

/** How a level reads: one of the three roles, or a number of its own. */
export type LevelChoice = 'Admin' | 'Moderator' | 'Member' | 'Custom';

/** The choice a level falls on. */
export function levelChoice(level: number): LevelChoice {
  switch (level) {
    case 100:
      return 'Admin';
    case 50:
      return 'Moderator';
    case 0:
      return 'Member';
    default:
      return 'Custom';
  }
}

/** The level a role choice gives. */
export const CHOICE_LEVELS: ReadonlyArray<[Exclude<LevelChoice, 'Custom'>, number]> = [
  ['Admin', 100],
  ['Moderator', 50],
  ['Member', 0],
];

/**
 * A typed custom level as a whole number the user may give (never above their own), or `null`
 * when it isn't a number.
 */
export function parseLevel(typed: string, ownLevel: number): number | null {
  const text = typed.trim();

  if (!/^-?\d+$/.test(text)) return null;
  const level = Number.parseInt(text, 10);

  // Matrix levels are integers JSON carries exactly.
  if (!Number.isSafeInteger(level)) return null;

  return Math.min(level, ownLevel);
}

/** Whether the user may change a permission: only one asking no more than their own level. */
export function permissionEditable(permission: PermissionLevel, ownLevel: number): boolean {
  return permission.editable && permission.level <= ownLevel;
}

/** The levels the user may pick from the roles: none above their own. */
export function choicesOffered(ownLevel: number): Array<[Exclude<LevelChoice, 'Custom'>, number]> {
  return CHOICE_LEVELS.filter(([, level]) => level <= ownLevel);
}

/** The change that sets `permission` to `level`, clamped to the user's own; `null` if unchanged. */
export function permissionSetting(
  settings: RoomSettings,
  permission: Permission,
  level: number,
): RoomSetting | null {
  const current = settings.permissions.find((p) => p.permission === permission);
  const clamped = Math.min(level, settings.own_level);

  // Send messages and Access show the highest of several levels: the same level may still raise
  // or lower one of them, so it is always sent.
  const several = permission === 'SendMessages' || permission === 'Access';

  if (!current || !permissionEditable(current, settings.own_level)) return null;

  if (current.level === clamped && !several) return null;

  return { type: 'Permission', data: { permission, level: clamped } };
}

// ----- a space's rooms -----------------------------------------------------------------------

/** `children` with `roomId` moved `delta` places, or `null` when it can't move that way. */
export function reorder(children: string[], roomId: string, delta: number): string[] | null {
  const from = children.indexOf(roomId);
  const to = from + delta;

  if (from < 0 || delta === 0 || to < 0 || to >= children.length) return null;
  const next = children.slice();
  next.splice(from, 1);
  next.splice(to, 0, roomId);

  return next;
}

/** The command that orders a space's rooms as `order` (every child, in its new place). */
export function changeSpaceCommand(
  account: AccountId,
  space: string,
  change: SpaceChange,
): CommandOf<'ChangeSpace'> {
  return { type: 'ChangeSpace', data: { account, space, change } };
}

/** One child of a space as the manage screen lists it. */
export interface SpaceChild {
  roomId: string;
  name: string;
  /** A subspace, not a room. */
  space: boolean;
  /** Whether the user is in it (the name comes from the room list then). */
  joined: boolean;
}

/** A space's children in its order, named where the account knows them. */
export function spaceChildren(
  space: SpaceSummary,
  rooms: RoomSummary[],
  spaces: SpaceSummary[],
): SpaceChild[] {
  return space.children.map((roomId) => {
    const room = rooms.find((r) => r.room_id === roomId);
    const sub = spaces.find((s) => s.room_id === roomId);

    return {
      roomId,
      name: room?.name ?? sub?.name ?? roomId,
      space: sub !== undefined,
      joined: room !== undefined || sub !== undefined,
    };
  });
}

/** The joined group rooms that may be added to a space: those it doesn't list yet. */
export function roomsToAdd(space: SpaceSummary, rooms: RoomSummary[]): RoomSummary[] {
  return rooms.filter(
    (r) => r.kind === 'Group' && r.successor === null && !space.children.includes(r.room_id),
  );
}

/** The space's rooms (and subspaces) the user is in: what leaving it may take along. */
export function joinedChildren(
  space: SpaceSummary,
  rooms: RoomSummary[],
  spaces: SpaceSummary[],
): SpaceChild[] {
  return spaceChildren(space, rooms, spaces).filter((c) => c.joined);
}

/** The command that leaves a space and the chosen rooms, in the space's order. */
export function leaveSpaceCommand(
  account: AccountId,
  space: SpaceSummary,
  chosen: ReadonlySet<string>,
): CommandOf<'LeaveSpace'> {
  return {
    type: 'LeaveSpace',
    data: { account, space: space.room_id, rooms: space.children.filter((id) => chosen.has(id)) },
  };
}

// ----- upgrades ------------------------------------------------------------------------------

/** How to reach an upgraded room's successor: open it when joined, else join it first. */
export function successorRoute(
  successor: Successor,
  rooms: RoomSummary[],
): { kind: 'open'; roomId: string } | { kind: 'join'; roomId: string; via: string[] } {
  return rooms.some((r) => r.room_id === successor.room_id)
    ? { kind: 'open', roomId: successor.room_id }
    : { kind: 'join', roomId: successor.room_id, via: successor.via };
}
