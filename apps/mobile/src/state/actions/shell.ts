// The shell's actions: the room list's sections (saved to the account as its `SidebarLayout`,
// as the desktop saves them), a room's notification mode, marking read and leaving.

import type { AccountId, NotificationMode, SidebarLayout } from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import { addNotice, updateAccount } from '../reducer';
import { CommandFailed } from '../requests';
import * as sections from '../sections';
import type { Store } from '../store';
import { roomKey } from '../timeline';
import type { Scope } from '../types';
import { releaseTimeline, retainTimeline, untilOpened } from './timeline';

/** Per store: accounts whose layout is being saved, and those changed again meanwhile. */
const saving = new WeakMap<Store, { inFlight: Set<AccountId>; unsaved: Set<AccountId> }>();

function savesOf(store: Store) {
  let saves = saving.get(store);
  if (!saves) {
    saves = { inFlight: new Set(), unsaved: new Set() };
    saving.set(store, saves);
  }
  return saves;
}

/**
 * Saves an account's layout. One save runs at a time per account; changes made meanwhile are
 * saved once it finished, with the layout as it is by then.
 */
function saveLayout(store: Store, account: AccountId): void {
  const saves = savesOf(store);
  if (saves.inFlight.has(account)) {
    saves.unsaved.add(account);
    return;
  }
  const layout = store.getState().accounts.find((a) => a.summary.id === account)?.sidebar;
  if (!layout) return;
  saves.inFlight.add(account);
  store
    .run({ type: 'SetSidebarLayout', data: { account, layout } })
    .catch((error: unknown) => {
      if (error instanceof CommandFailed && error.cancelled) return;
      const message = error instanceof Error ? error.message : String(error);
      store.update((s) => addNotice(s, t('notice.layout', { error: message }), account));
    })
    .finally(() => {
      saves.inFlight.delete(account);
      if (saves.unsaved.delete(account)) saveLayout(store, account);
    });
}

/**
 * Changes an account's layout for `scope`, shows it at once and saves it. Refused (with a notice)
 * until the saved layout has arrived: saving before would replace it on every device.
 */
export function changeLayout(
  store: Store,
  account: AccountId,
  change: (layout: SidebarLayout) => SidebarLayout,
  now: number = Date.now(),
): boolean {
  const data = store.getState().accounts.find((a) => a.summary.id === account);
  if (!data) return false;
  if (!data.sidebarLoaded) {
    store.update((s) => addNotice(s, t('notice.layout_loading'), account));
    return false;
  }
  const layout = sections.stamped(change(data.sidebar), data.sidebar, now);
  store.update((s) => updateAccount(s, account, (a) => ({ ...a, sidebar: layout })));
  saveLayout(store, account);
  return true;
}

/** Collapses a section of the room list, or expands it again. */
export function toggleSection(store: Store, account: AccountId, scope: Scope, id: string): void {
  changeLayout(store, account, (l) => sections.toggleCollapsed(l, scope, id));
}

/** Adds a section, first in the scope; with `roomId`, moves that room into it. */
export function addSection(store: Store, account: AccountId, scope: Scope, name: string, roomId?: string): void {
  if (!name.trim()) return;
  changeLayout(store, account, (l) => {
    const added = sections.addSection(l, scope, name);
    return roomId ? sections.moveRoom(added.layout, scope, roomId, added.id) : added.layout;
  });
}

export function renameSection(store: Store, account: AccountId, scope: Scope, id: string, name: string): void {
  if (!name.trim()) return;
  changeLayout(store, account, (l) => sections.renameSection(l, scope, id, name));
}

/** Removes a section of the user's; its rooms go back to Rooms and Direct messages. */
export function deleteSection(store: Store, account: AccountId, scope: Scope, id: string): void {
  changeLayout(store, account, (l) => sections.deleteSection(l, scope, id));
}

/**
 * Moves a room to the end of one of the user's sections, or out of them with a built-in
 * section's ID (the phone's form of the desktop's drag and drop).
 */
export function moveRoomToSection(store: Store, account: AccountId, scope: Scope, roomId: string, section: string): void {
  changeLayout(store, account, (l) => sections.moveRoom(l, scope, roomId, section));
}

/**
 * Sets when a room notifies (`null` follows the account's default), shown at once and confirmed
 * by the next room list.
 */
export function setNotificationMode(store: Store, account: AccountId, roomId: string, mode: NotificationMode | null): void {
  store.update((s) =>
    updateAccount(s, account, (a) => ({
      ...a,
      rooms: a.rooms.map((room) =>
        room.room_id === roomId
          ? { ...room, notifications: mode ?? a.summary.default_notifications, custom_notifications: mode !== null }
          : room,
      ),
    })),
  );
  store.send({ type: 'SetNotificationMode', data: { account, room_id: roomId, mode } });
}

/**
 * Marks a room read up to its newest message, from the room list: the engine marks through an
 * open timeline, so the room's opens for it (unless a screen shows it already) and closes after.
 */
export async function markRoomRead(store: Store, account: AccountId, roomId: string): Promise<void> {
  const key = roomKey(roomId);
  retainTimeline(store, account, key);
  try {
    if (!(await untilOpened(store, account, key))) return;
    await store.run({ type: 'MarkRead', data: { account, key, event_id: null, fully_read: true } });
  } catch {
    // The room stays unread, as it shows.
  } finally {
    releaseTimeline(store, account, key);
  }
}

/** Leaves a room; `forget` also forgets it on the server, as deleting a direct chat does. */
export function leaveRoom(store: Store, account: AccountId, roomId: string, forget: boolean): void {
  store.send({ type: 'LeaveRoom', data: { account, room_id: roomId, forget } });
}
