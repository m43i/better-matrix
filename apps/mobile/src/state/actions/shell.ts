// The shell's actions: the room list's sections (saved to the account as its `SidebarLayout`,
// as the desktop saves them), a room's notification mode, its place among the favourites and
// low priority rooms, marking it read or unread, and leaving.

import type {
  AccountId,
  Command,
  NotificationMode,
  RoomPriority,
  RoomSummary,
  SidebarLayout,
} from '@better-matrix/matrix-native/types';

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
    .catch((cause: unknown) => {
      if (cause instanceof CommandFailed && cause.cancelled) return;
      const message = cause instanceof Error ? cause.message : String(cause);
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
export function addSection(
  store: Store,
  account: AccountId,
  scope: Scope,
  name: string,
  roomId?: string,
) {
  if (!name.trim()) return;

  const moved = changeLayout(store, account, (l) => {
    const added = sections.addSection(l, scope, name);

    return roomId ? sections.moveRoom(added.layout, scope, roomId, added.id) : added.layout;
  });

  // A favourite or low priority room moved into a section of the user's loses its tag, so it
  // shows there.
  if (moved && roomId) setRoomPriority(store, account, roomId, 'Normal');
}

export function renameSection(
  store: Store,
  account: AccountId,
  scope: Scope,
  id: string,
  name: string,
): void {
  if (!name.trim()) return;
  changeLayout(store, account, (l) => sections.renameSection(l, scope, id, name));
}

/** Removes a section of the user's; its rooms go back to Rooms and Direct messages. */
export function deleteSection(store: Store, account: AccountId, scope: Scope, id: string): void {
  changeLayout(store, account, (l) => sections.deleteSection(l, scope, id));
}

/**
 * Moves a room to the end of one of the user's sections, or out of them with a built-in
 * section's ID (the phone's form of the desktop's drag and drop). Moved into Favourites or Low
 * priority it takes that tag; moved anywhere else it loses it, so it shows where it was moved.
 */
export function moveRoomToSection(
  store: Store,
  account: AccountId,
  scope: Scope,
  roomId: string,
  section: string,
): void {
  // Into Favourites or Low priority only the tag changes: a section of the user's that holds the
  // room keeps it for when the tag goes.
  if (section === sections.FAVOURITES || section === sections.LOW_PRIORITY) {
    setRoomPriority(store, account, roomId, sections.priorityIn(section));

    return;
  }

  // The tag follows only a move the layout takes.
  if (changeLayout(store, account, (l) => sections.moveRoom(l, scope, roomId, section))) {
    setRoomPriority(store, account, roomId, sections.priorityIn(section));
  }
}

/** Changes one room of an account here, ahead of the engine's room list. */
function changeRoom(
  store: Store,
  account: AccountId,
  roomId: string,
  change: (room: RoomSummary) => RoomSummary,
): void {
  store.update((s) =>
    updateAccount(s, account, (a) => ({
      ...a,
      rooms: a.rooms.map((room) => (room.room_id === roomId ? change(room) : room)),
    })),
  );
}

/**
 * Runs a command marking or moving a room; a failure takes back the change (`undo`, which keeps
 * whatever else changed about the room since) and tells the user (`notice.room_mark`).
 */
function markRoom(
  store: Store,
  account: AccountId,
  roomId: string,
  undo: (room: RoomSummary) => RoomSummary,
  command: Command,
): void {
  store.run(command).catch((cause: unknown) => {
    if (cause instanceof CommandFailed && cause.cancelled) return;
    const message = cause instanceof Error ? cause.message : String(cause);
    changeRoom(store, account, roomId, undo);
    store.update((s) => addNotice(s, t('notice.room_mark', { error: message }), account));
  });
}

/** One of an account's rooms, as the store has it now. */
function roomOf(store: Store, account: AccountId, roomId: string): RoomSummary | undefined {
  return store
    .getState()
    .accounts.find((a) => a.summary.id === account)
    ?.rooms.find((r) => r.room_id === roomId);
}

/**
 * Puts a room among the favourites or the low priority rooms, or neither, on every client
 * (room tags); shown at once. Nothing is sent when the room is there already.
 */
export function setRoomPriority(
  store: Store,
  account: AccountId,
  roomId: string,
  priority: RoomPriority,
): void {
  const room = roomOf(store, account, roomId);

  if (!room || room.priority === priority) return;
  changeRoom(store, account, roomId, (r) => ({ ...r, priority }));
  markRoom(store, account, roomId, (r) => ({ ...r, priority: room.priority }), {
    type: 'SetRoomPriority',
    data: { account, room_id: roomId, priority },
  });
}

/**
 * Marks a room unread (MSC2867) on every session, or takes the mark away, shown at once. Nothing
 * is read: `markRoomRead` reads a room with unread messages.
 */
export function setUnread(store: Store, account: AccountId, roomId: string, unread: boolean): void {
  const room = roomOf(store, account, roomId);

  if (!room) return;
  changeRoom(store, account, roomId, (r) => ({ ...r, marked_unread: unread }));
  markRoom(store, account, roomId, (r) => ({ ...r, marked_unread: room.marked_unread }), {
    type: 'SetUnread',
    data: { account, room_id: roomId, unread },
  });
}

/**
 * A room was opened: one the user marked unread loses the mark on every session (its read
 * receipt follows as the timeline shows it).
 */
export function unmarkOpened(store: Store, account: AccountId, roomId: string): void {
  const room = roomOf(store, account, roomId);

  if (!room?.marked_unread) return;
  changeRoom(store, account, roomId, (r) => ({ ...r, marked_unread: false }));
  markRoom(store, account, roomId, (r) => ({ ...r, marked_unread: true }), {
    type: 'SetUnread',
    data: { account, room_id: roomId, unread: false },
  });
}

/**
 * Sets when a room notifies (`null` follows the account's default), shown at once and confirmed
 * by the next room list.
 */
export function setNotificationMode(
  store: Store,
  account: AccountId,
  roomId: string,
  mode: NotificationMode | null,
): void {
  store.update((s) =>
    updateAccount(s, account, (a) => ({
      ...a,
      rooms: a.rooms.map((room) =>
        room.room_id === roomId
          ? {
              ...room,
              notifications: mode ?? a.summary.default_notifications,
              custom_notifications: mode !== null,
            }
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
export async function markRoomRead(
  store: Store,
  account: AccountId,
  roomId: string,
): Promise<void> {
  const key = roomKey(roomId);
  retainTimeline(store, account, key);

  try {
    if (!(await untilOpened(store, account, key))) return;
    await store.run({ type: 'MarkRead', data: { account, key, event_id: null, fully_read: true } });

    // A mark the user set goes too, on the server as well: the receipt may not have been needed.
    if (roomOf(store, account, roomId)?.marked_unread) setUnread(store, account, roomId, false);
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
