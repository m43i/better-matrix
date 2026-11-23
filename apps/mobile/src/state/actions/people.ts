// People and rooms: a room's details (members, threads, files), what the user does to members,
// inviting, direct chats, making and renaming rooms and spaces, leaving, notification modes, and
// searching the user directory as it is typed.

import type {
  AccountId,
  Attachment,
  Command,
  Moderation,
  NotificationMode,
  RequestId,
} from '@better-matrix/matrix-native/types';

import { type Key, t } from '../../i18n';
import { addNotice, updateAccount } from '../reducer';
import {
  type CommandOf,
  EMPTY_SEARCH,
  type PeopleSearch,
  SEARCH_PAUSE_MS,
  due,
  finished,
  found,
  searching,
  typed,
  withNotificationMode,
} from '../people';
import { CommandFailed } from '../requests';
import { mediaUri } from '../selectors';
import type { Store } from '../store';
import { setScope } from './core';

/**
 * Runs `command`; when it fails, tells the user in the desktop's words (`notice` takes the
 * error as `{{error}}`) and rejects. A cancelled command rejects without a word.
 */
export async function attempt(
  store: Store,
  command: Command,
  notice: Key,
  onEvent?: Parameters<Store['run']>[1],
): Promise<void> {
  try {
    await store.run(command, onEvent);
  } catch (error) {
    if (!(error instanceof CommandFailed)) throw error;
    const failed = error;

    if (!failed.cancelled) store.update((s) => addNotice(s, t(notice, { error: failed.message })));
    throw error;
  }
}

// ----- a room's details ----------------------------------------------------------------------

/** Fetches everyone in a room, unless the account's list already has them all. */
export function listAllMembers(store: Store, account: AccountId, roomId: string): void {
  const known = store.getState().accounts.find((a) => a.summary.id === account)?.members[roomId];

  if (known?.complete) return;
  store.send(
    { type: 'ListMembers', data: { account, room_id: roomId, all: true } },
    { quiet: true },
  );
}

/** Fetches a room's threads afresh. A failure ends the loading with an empty list, and says why. */
export function listThreads(store: Store, account: AccountId, roomId: string): void {
  attempt(store, { type: 'ListThreads', data: { account, room_id: roomId } }, 'notice.side').catch(
    () => {
      store.update((s) =>
        updateAccount(s, account, (a) => ({
          ...a,
          threads: { ...a.threads, [roomId]: a.threads[roomId] ?? [] },
        })),
      );
    },
  );
}

/** Fetches the files shared in a room afresh. A failure ends the loading, and says why. */
export function listFiles(store: Store, account: AccountId, roomId: string): void {
  attempt(store, { type: 'ListFiles', data: { account, room_id: roomId } }, 'notice.side').catch(
    () => {
      store.update((s) =>
        updateAccount(s, account, (a) => ({
          ...a,
          files: { ...a.files, [roomId]: a.files[roomId] ?? [] },
        })),
      );
    },
  );
}

/**
 * A shared file's local `file://` URI, fetched in full first unless it is already there, for the
 * system's share sheet. `undefined` when the file has no media or the fetch failed.
 */
export async function fileForSharing(
  store: Store,
  account: AccountId,
  attachment: Attachment,
): Promise<string | undefined> {
  const media = attachment.media;

  if (!media) return undefined;
  const original = { type: 'Original' } as const;
  const have = mediaUri(store.getState(), media, original);

  if (have) return have;

  try {
    await attempt(
      store,
      { type: 'FetchMedia', data: { account, media, size: original } },
      'notice.save',
    );
  } catch {
    return undefined;
  }

  return mediaUri(store.getState(), media, original);
}

// ----- the room itself -----------------------------------------------------------------------

/**
 * Sets a room's own notification mode, or with `null` makes it follow the account's default.
 * The room shows it at once; the engine's room list confirms it.
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
      rooms: a.rooms.map((r) =>
        r.room_id === roomId ? withNotificationMode(r, mode, a.summary.default_notifications) : r,
      ),
    })),
  );
  attempt(
    store,
    { type: 'SetNotificationMode', data: { account, room_id: roomId, mode } },
    'notice.notification_setting',
  ).catch(() => {});
}

/** Leaves a room; `forget` also deletes it on the server (a direct chat). */
export function leaveRoom(
  store: Store,
  account: AccountId,
  roomId: string,
  forget: boolean,
): Promise<void> {
  return attempt(
    store,
    { type: 'LeaveRoom', data: { account, room_id: roomId, forget } },
    'notice.leave',
  );
}

/** Makes a room or space and answers with its ID once the engine made it. */
export async function create(store: Store, command: CommandOf<'CreateRoom'>): Promise<string> {
  return madeRoom(store, command, 'notice.create');
}

/** Runs a command the engine answers with a new room (`RoomCreated`); resolves to its ID. */
export async function madeRoom(store: Store, command: Command, notice: Key): Promise<string> {
  let roomId: string | null = null;
  await attempt(store, command, notice, (event) => {
    if (event.type === 'RoomCreated') roomId = event.data.room_id;
  });

  if (roomId === null) throw new Error('the engine made no room');

  return roomId;
}

/** Makes a space and shows it in the account's room list. */
export async function createSpace(store: Store, command: CommandOf<'CreateRoom'>): Promise<string> {
  const spaceId = await create(store, command);
  setScope(store, command.data.account, { space: spaceId });

  return spaceId;
}

// ----- people --------------------------------------------------------------------------------

/**
 * Kicks or bans a member, gives them a role or level, unbans someone, or approves or denies a
 * request to join. Confirming first is the screen's part.
 */
export function moderate(
  store: Store,
  account: AccountId,
  roomId: string,
  userId: string,
  action: Moderation,
): Promise<void> {
  return attempt(
    store,
    { type: 'ModerateMember', data: { account, room_id: roomId, user_id: userId, action } },
    'notice.moderate',
  );
}

/** Invites someone to a room. */
export function invite(
  store: Store,
  account: AccountId,
  roomId: string,
  userId: string,
): Promise<void> {
  return attempt(
    store,
    { type: 'Invite', data: { account, room_id: roomId, user_id: userId } },
    'notice.invite',
  );
}

/** The direct chat with someone, started when there is none: answers with its room ID. */
export function openDirectChat(store: Store, account: AccountId, userId: string): Promise<string> {
  return openChat(store, { type: 'OpenDirectChat', data: { account, user_id: userId } });
}

/**
 * The user's note to self, made (named in their language) when there is none: answers with its
 * room ID. Only the searches that offer it open it.
 */
export function openNoteToSelf(store: Store, account: AccountId): Promise<string> {
  return openChat(store, {
    type: 'OpenNoteToSelf',
    data: { account, name: t('note_to_self.name') },
  });
}

/** The chat `command` opens, as `Event::DirectChat` answers it. */
async function openChat(store: Store, command: Command): Promise<string> {
  let roomId: string | null = null;

  await attempt(store, command, 'notice.direct_chat', (event) => {
    if (event.type === 'DirectChat') roomId = event.data.room_id;
  });

  if (roomId === null) throw new Error('the engine opened no chat');

  return roomId;
}

/**
 * Searching one account's user directory as it is typed: suggestions first, then a search once
 * the text rests for `SEARCH_PAUSE_MS`; only the latest search's answer counts. Screens hold one
 * per search field (`usePeopleSearch`) and hear each change through `onChange`.
 */
export class PeopleSearcher {
  private search: PeopleSearch = EMPTY_SEARCH;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;

  constructor(
    private readonly store: Store,
    private readonly account: AccountId,
    private readonly onChange: (search: PeopleSearch) => void,
  ) {}

  get current(): PeopleSearch {
    return this.search;
  }

  /** Asks whom to suggest before anything is typed. */
  suggest(): void {
    const request = this.ask({ type: 'SuggestPeople', data: this.account });
    this.set({ ...this.search, suggesting: request });
  }

  /** The text changed: its search goes out once it rests. */
  type(text: string): void {
    this.set(typed(this.search, text));

    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;

    if (due(this.search) === null) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      const term = due(this.search);

      if (term === null) return;
      const request = this.ask({ type: 'SearchPeople', data: { account: this.account, term } });
      this.set(searching(this.search, term, request));
    }, SEARCH_PAUSE_MS);
  }

  /** Stops searching: no later answer changes anything. */
  close(): void {
    this.closed = true;

    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  private ask(command: Command): RequestId {
    const done = this.store.run(command, (event) => {
      if (event.type === 'PeopleFound')
        this.answer(found(this.search, event.data.request, event.data.people));
    });

    const request = done.request;
    // A failed search finds nobody; the user can type again.
    const end = () => this.answer(finished(this.search, request));
    done.then(end, end);

    return request;
  }

  private answer(next: PeopleSearch): void {
    if (!this.closed) this.set(next);
  }

  private set(next: PeopleSearch): void {
    if (next === this.search) return;
    this.search = next;
    this.onChange(next);
  }
}
