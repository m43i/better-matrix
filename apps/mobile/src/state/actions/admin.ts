// Room and space administration: loading and changing a room's settings, upgrading it, arranging
// a space's rooms, leaving a space with some of its rooms, and going to an upgraded room's
// successor (ADR 0008). Failures become notices; the engine's fresh `RoomSettings` and room list
// bring every change into the state.

import type {
  AccountId,
  RoomSetting,
  SpaceChange,
  SpaceSummary,
  Successor,
} from '@better-matrix/matrix-native/types';

import { changeRoomCommand, changeSpaceCommand, leaveSpaceCommand, successorRoute } from '../admin';
import { updateAccount } from '../reducer';
import { account as accountOf } from '../selectors';
import type { Store } from '../store';
import { joinRoom } from './links';
import { attempt, madeRoom } from './people';

/** Asks for a room's or space's settings; they arrive as `RoomSettings`. */
export function loadRoomSettings(store: Store, account: AccountId, roomId: string): void {
  attempt(
    store,
    { type: 'LoadRoomSettings', data: { account, room_id: roomId } },
    'mobile.admin.notice.load',
  ).catch(() => {});
}

/** Changes one setting; resolves once the engine reported the room with it (or failed). */
export function changeRoom(
  store: Store,
  account: AccountId,
  roomId: string,
  setting: RoomSetting,
): Promise<void> {
  return attempt(store, changeRoomCommand(account, roomId, setting), 'mobile.admin.notice.change');
}

/** Upgrades a room to `version`; resolves to the new room's ID. */
export function upgradeRoom(
  store: Store,
  account: AccountId,
  roomId: string,
  version: string,
): Promise<string> {
  return madeRoom(
    store,
    { type: 'UpgradeRoom', data: { account, room_id: roomId, version } },
    'mobile.admin.notice.upgrade',
  );
}

/** Adds, removes or orders a space's rooms. */
export function changeSpace(
  store: Store,
  account: AccountId,
  space: string,
  change: SpaceChange,
): Promise<void> {
  return attempt(store, changeSpaceCommand(account, space, change), 'mobile.admin.notice.space');
}

/**
 * Puts a space's rooms in `order` (all of them): the list shows it at once, and the engine's room
 * list confirms it. After a failure the order shown before comes back, unless something changed
 * it since (a later move, or the engine's list).
 */
export function orderSpace(
  store: Store,
  account: AccountId,
  space: string,
  order: string[],
): Promise<void> {
  const before = accountOf(store.getState(), account)?.spaces.find((s) => s.room_id === space);
  setChildren(store, account, space, order);

  return changeSpace(store, account, space, { type: 'Order', data: order }).catch((error) => {
    const now = accountOf(store.getState(), account)?.spaces.find((s) => s.room_id === space);
    const unchanged = now !== undefined && now.children.join('\n') === order.join('\n');

    if (before && unchanged) setChildren(store, account, space, before.children);
    throw error;
  });
}

function setChildren(store: Store, account: AccountId, space: string, children: string[]): void {
  store.update((s) =>
    updateAccount(s, account, (a) => ({
      ...a,
      spaces: a.spaces.map((sp) => (sp.room_id === space ? { ...sp, children } : sp)),
    })),
  );
}

/** Leaves a space and, first, the chosen ones of its rooms. */
export function leaveSpace(
  store: Store,
  account: AccountId,
  space: SpaceSummary,
  rooms: ReadonlySet<string>,
): Promise<void> {
  return attempt(store, leaveSpaceCommand(account, space, rooms), 'notice.leave');
}

/** The room that replaced an upgraded one, joined first when the user isn't in it: its ID. */
export async function reachSuccessor(
  store: Store,
  account: AccountId,
  successor: Successor,
): Promise<string> {
  const route = successorRoute(successor, accountOf(store.getState(), account)?.rooms ?? []);

  if (route.kind === 'open') return route.roomId;

  return joinRoom(store, account, route.roomId, route.via);
}
