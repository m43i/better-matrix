// Derived data views read: kept here so every screen derives it the same way.

import type {
  AccountId,
  MediaRef,
  Message,
  MediaSize,
  MessagePreview,
  RoomPreview,
  RoomSummary,
  SavedMessage,
  SharedFile,
  ThreadInfo,
  TimelineEntry,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { galleryAround, thumbnailOf } from './gallery';
import { EMPTY_TIMELINE, mediaKey } from './reducer';
import { hiddenEntry, revealKey, showsMedia, shownInvitations } from './safety';
import { newestMessages, type NewestMessages } from './sections';
import { timelineId } from './timeline';
import type { AccountData, State, Timeline } from './types';

export function account(state: State, id: AccountId | null): AccountData | undefined {
  return id === null ? undefined : state.accounts.find((a) => a.summary.id === id);
}

/** The account on screen. */
export function activeAccount(state: State): AccountData | undefined {
  return account(state, state.active);
}

/**
 * What the room lists (the Home and DMs tabs) draw from an account: of its timelines, only each
 * room's newest message (`newestMessages`).
 */
export type RoomListData = Pick<
  AccountData,
  | 'summary'
  | 'rooms'
  | 'spaces'
  | 'invitations'
  | 'roomsLoaded'
  | 'scope'
  | 'sidebar'
  | 'latest'
  | 'heardAt'
> & { newest: NewestMessages };

/**
 * The account on screen as the room lists draw it. Compared with `shallow`, it changes only when
 * one of these fields does: typing, members, threads and the like leave the lists alone, and so
 * does a timeline changing anything but a room's newest message as the list shows it.
 */
export function activeRoomList(state: State): RoomListData | undefined {
  return roomListOf(state, state.active);
}

/** An account as the room lists draw it (`activeRoomList`), search's rooms included. */
export function roomListOf(state: State, id: AccountId | null): RoomListData | undefined {
  const data = account(state, id);

  if (!data) return undefined;

  return {
    summary: data.summary,
    rooms: data.rooms,
    spaces: data.spaces,
    // Strangers' invitations the user hides stay out of the list and its counter.
    invitations: shownInvitations(data.invitations, data.safety),
    roomsLoaded: data.roomsLoaded,
    scope: data.scope,
    sidebar: data.sidebar,
    // An ignore still on its way hides its person's previews here, so a failed one gives them
    // back; a confirmed one removed them from the state (`reducer.withSafety`).
    newest: withoutSenders(newestMessages(data), pendingIgnored(data)),
    latest: withoutSenders(data.latest, pendingIgnored(data)),
    heardAt: data.heardAt,
  };
}

/**
 * Everyone whose messages and hidden events the timeline leaves out: those shown as ignored, those
 * the engine still has as ignored (an ignore being undone, until that is saved), and anyone an
 * Ignore on its way names, even when a later Stop ignoring is on its way too: content shows again
 * only once no Ignore for them is pending and the engine no longer has them.
 */
export function shownIgnored(data: AccountData | undefined): string[] | undefined {
  if (!data) return undefined;

  const pending = data.pendingSafety.flatMap(({ change }) =>
    change.type === 'Ignore' && change.data.ignored ? [change.data.user_id] : [],
  );

  const extra = [...data.savedSafety.ignored, ...pending].filter(
    (id, i, all) => !data.safety.ignored.includes(id) && all.indexOf(id) === i,
  );

  return extra.length === 0 ? data.safety.ignored : [...data.safety.ignored, ...extra];
}

/**
 * Whether what `userId` sent stays out of sight: ignored as shown, or still as the engine has it
 * while stopping is on its way (`shownIgnored`), so a pending Stop ignoring shows nothing early.
 */
export function hides(data: AccountData, userId: string): boolean {
  return shownIgnored(data)?.includes(userId) ?? false;
}

/**
 * The people hidden (`shownIgnored`) whom the engine hasn't confirmed as ignored: shown as
 * ignored, or named by an Ignore on its way, a later Stop ignoring on its way too included. What
 * they sent is hidden as it is read; what confirmed ignores' people sent left the state already.
 */
export function pendingIgnored(data: AccountData): string[] {
  return (shownIgnored(data) ?? []).filter((id) => !data.savedSafety.ignored.includes(id));
}

/** `messages` without those `senders` sent; the same object when there are none. */
function withoutSenders(
  messages: Record<string, Message>,
  senders: readonly string[],
): Record<string, Message> {
  if (senders.length === 0) return messages;

  return Object.fromEntries(
    Object.entries(messages).filter(([, message]) => !senders.includes(message.sender_id)),
  );
}

/** Each timeline as `shownTimeline` last gave it, with the pending ignores it hid. */
const shownTimelines = new WeakMap<Timeline, { hidden: string; shown: Timeline }>();

/**
 * A timeline as its rows draw it: what people whose ignore is still on its way said draws as
 * nothing, so a failed ignore gives it back. The same object while nothing changes.
 */
export function shownTimeline(state: State, id: AccountId, key: TimelineKey): Timeline {
  const kept = timeline(state, id, key);
  const data = account(state, id);
  const pending = data ? pendingIgnored(data) : [];

  if (pending.length === 0) return kept;
  const hidden = pending.join(',');
  const cached = shownTimelines.get(kept);

  if (cached?.hidden === hidden) return cached.shown;

  const entries = kept.entries.map((entry): TimelineEntry =>
    entry.kind.type === 'Message' && pending.includes(entry.kind.data.sender_id)
      ? hiddenEntry(entry.id, entry.kind.data)
      : entry,
  );

  const shown = { ...kept, entries };
  shownTimelines.set(kept, { hidden, shown });

  return shown;
}

export function timeline(state: State, id: AccountId, key: TimelineKey): Timeline {
  return account(state, id)?.timelines[timelineId(key)] ?? EMPTY_TIMELINE;
}

export function room(state: State, id: AccountId, roomId: string) {
  return account(state, id)?.rooms.find((r) => r.room_id === roomId);
}

/** A fetched media file's `file://` URI, once it's there. */
const NO_ONE: string[] = [];

/**
 * Whom chat bubbles leave unnamed in `room`: the other person of a one-to-one direct chat, where
 * who wrote what is plain. Once someone else is there, or for anyone who was and left, names
 * show.
 */
export function unnamedSenders(room: RoomSummary | undefined): string[] {
  return room?.kind === 'Direct' && room.member_count <= 2 ? room.dm_users : NO_ONE;
}

export function mediaUri(state: State, media: MediaRef, size: MediaSize): string | undefined {
  const path = state.media[mediaKey(media, size)];

  return path === undefined ? undefined : `file://${path}`;
}

/**
 * The saved messages listed, as the rooms' summaries know them now: one no longer saved leaves
 * at once. `undefined` until listed.
 */
export function savedMessages(state: State, id: AccountId): SavedMessage[] | undefined {
  const data = account(state, id);

  if (!data?.saved) return undefined;

  const saved = new Set(
    data.rooms.flatMap((r) => r.saved.map((eventId) => `${r.room_id}|${eventId}`)),
  );

  // What people the user ignores said stays out of the list; the saved IDs stay as they are.
  return data.saved.filter(
    (m) => saved.has(`${m.room_id}|${m.preview.event_id}`) && !hides(data, m.preview.sender_id),
  );
}

/**
 * A room's pinned messages as its list shows them: those still pinned, without what people the
 * user ignores said. The room's pins themselves stay as they are.
 */
export function pinnedPreviews(
  state: State,
  id: AccountId,
  roomId: string,
): MessagePreview[] | undefined {
  const data = account(state, id);
  const pins = data?.pins[roomId];

  if (!data || !pins) return undefined;
  const pinned = data.rooms.find((r) => r.room_id === roomId)?.pinned ?? [];

  return pins.filter((pin) => pinned.includes(pin.event_id) && !hides(data, pin.sender_id));
}

/** Every saved message's room and event, as one string: it changes when anything is saved. */
export function savedSignature(state: State, id: AccountId): string {
  return (account(state, id)?.rooms ?? [])
    .flatMap((r) => r.saved.map((eventId) => `${r.room_id}|${eventId}`))
    .sort()
    .join(',');
}

/**
 * Whether a message's pictures and videos wait for a tap on Show (ADR 0011): the account's media
 * choice, the room's being public and whose message it is, unless the user showed it already.
 */
export function mediaHidden(
  state: State,
  id: AccountId,
  roomId: string,
  message: { sender_id: string; event_id: string },
): boolean {
  const data = account(state, id);

  if (!data || state.revealedMedia[revealKey(id, message.event_id)]) return false;
  // A room with no summary yet counts as public: held under "private rooms only".
  const publicRoom = data.rooms.find((r) => r.room_id === roomId)?.public ?? true;

  return !showsMedia(data.safety, publicRoom, message.sender_id === id);
}

/**
 * Whether the attachment preview keeps a message's media back: `mediaHidden`, for pictures and
 * videos only. Audio and other files never wait for Show (ADR 0011).
 */
export function previewHidden(
  state: State,
  id: AccountId,
  roomId: string,
  message: Message,
): boolean {
  const content = message.content;

  const visual =
    content.type !== 'Attachment' || content.data.kind === 'Image' || content.data.kind === 'Video';

  return visual && mediaHidden(state, id, roomId, message);
}

/** Each listed file as the preview shows it, made once per file so views see the same object. */
const listedMessages = new WeakMap<SharedFile, Message>();

/**
 * A file Room Info lists, as a message the preview can show when no timeline holds it, gated as
 * its sender's (the user's own show).
 */
function listedMessage(file: SharedFile): Message {
  let message = listedMessages.get(file);

  if (!message) {
    message = {
      event_id: file.event_id,
      sender_id: file.sender_id,
      sender_name: file.sender_name,
      sender_avatar: null,
      content: { type: 'Attachment', data: file.attachment },
      edited: false,
      editable: false,
      pending: false,
      timestamp_ms: file.timestamp,
      reactions: [],
      reply_to: null,
      thread: null,
    };
    listedMessages.set(file, message);
  }

  return message;
}

/**
 * The attachment preview's gallery around `current`: the batch, media the account keeps back
 * included, each shown hidden until the user taps Show (`previewHidden`), whichever screen
 * opened it (ADR 0011). A file Room Info listed
 * that no timeline holds is its own gallery.
 */
export function previewItems(
  state: State,
  id: AccountId,
  key: TimelineKey,
  current: string,
  opened: string,
): Message[] {
  const data = account(state, id);
  const gallery = galleryAround(timeline(state, id, key).entries, current, opened, key.thread);
  // Opened from Room Info's files, the message may be held only by one of the room's threads.
  const found = gallery.length > 0 || !data ? gallery : inThreads(data, key, current, opened);

  if (found.length === 0) {
    // Only an event no timeline holds in any form (deleted, or hidden as its sender is
    // ignored) is rebuilt from the file Room Info listed, and never one an ignored person sent.
    const file = data?.files[key.room_id]?.find((f) => f.event_id === opened);

    return file && data && !hides(data, file.sender_id) && !heldAnywhere(data, key.room_id, opened)
      ? [listedMessage(file)]
      : [];
  }

  // Held items stay in the batch: their thumbnails fetch nothing (`galleryThumbnail`), and the
  // preview shows Show in place of one selected (`previewHidden`). What ignored people sent
  // leaves it, an ignore still on its way included (`hides`).
  return data && found.some((m) => hides(data, m.sender_id))
    ? found.filter((m) => !hides(data, m.sender_id))
    : found;
}

/**
 * Whether the preview's item `eventId` was sent by someone the user now ignores (`hides`): the
 * preview then shows nothing for it, rather than stepping to another item.
 */
export function previewIgnored(
  state: State,
  id: AccountId,
  key: TimelineKey,
  eventId: string,
): boolean {
  const data = account(state, id);

  const shown = galleryAround(timeline(state, id, key).entries, eventId, eventId, key.thread).find(
    (m) => m.event_id === eventId,
  );

  return data !== undefined && shown !== undefined && hides(data, shown.sender_id);
}

/**
 * The message the attachment preview shows for `eventId`, found as the preview finds it (in a
 * thread, or rebuilt from Room Info's files): what its More sheet acts on.
 */
export function previewMessage(
  state: State,
  id: AccountId,
  key: TimelineKey,
  eventId: string,
): Message | undefined {
  return previewItems(state, id, key, eventId, eventId).find((m) => m.event_id === eventId);
}

/** The gallery around `opened` in whichever of the room's kept thread timelines holds it. */
function inThreads(
  data: AccountData,
  key: TimelineKey,
  current: string,
  opened: string,
): Message[] {
  for (const [id, kept] of Object.entries(data.timelines)) {
    const root = id.startsWith(`${key.room_id}|`) ? id.slice(key.room_id.length + 1) : null;

    if (root === null || root === key.thread) continue;
    const gallery = galleryAround(kept.entries, current, opened, root);

    if (gallery.length > 0) return gallery;
  }

  return [];
}

/** A list as Room Info last showed it, by the list, with the ignore list it hid. */
type ShownLists<T> = WeakMap<T[], { ignored: string; shown: T[] }>;

const shownFileLists: ShownLists<SharedFile> = new WeakMap();

const shownThreadLists: ShownLists<ThreadInfo> = new WeakMap();

/**
 * `list` without what people the user ignores (as shown, so a pending ignore hides it too) shared
 * or started. The same array while neither changes.
 */
function withoutIgnoredSenders<T extends { sender_id: string }>(
  list: T[],
  ignored: string[],
  cache: ShownLists<T>,
): T[] {
  if (ignored.length === 0) return list;
  const key = ignored.join(',');
  const cached = cache.get(list);

  if (cached?.ignored === key) return cached.shown;
  const shown = list.filter((item) => !ignored.includes(item.sender_id));
  cache.set(list, { ignored: key, shown });

  return shown;
}

/** The files Room Info lists for a room, without what people the user ignores shared. */
export function shownFiles(state: State, id: AccountId, roomId: string): SharedFile[] | undefined {
  const data = account(state, id);
  const files = data?.files[roomId];

  return data && files
    ? withoutIgnoredSenders(files, shownIgnored(data) ?? [], shownFileLists)
    : files;
}

/** The threads Room Info lists for a room, without those people the user ignores started. */
export function shownThreads(
  state: State,
  id: AccountId,
  roomId: string,
): ThreadInfo[] | undefined {
  const data = account(state, id);
  const threads = data?.threads[roomId];

  return data && threads
    ? withoutIgnoredSenders(threads, shownIgnored(data) ?? [], shownThreadLists)
    : threads;
}

/** Whether any of a room's kept timelines holds `eventId`, or held it from an ignored sender. */
function heldAnywhere(data: AccountData, roomId: string, eventId: string): boolean {
  // Hidden as its sender is ignored: only while they still are, so stopping gives it back.
  const hiddenFrom = data.hiddenEvents[eventId];

  if (hiddenFrom !== undefined && hides(data, hiddenFrom)) return true;

  return Object.entries(data.timelines).some(
    ([id, kept]) =>
      (id === roomId || id.startsWith(`${roomId}|`)) &&
      kept.entries.some((e) => e.kind.type === 'Message' && e.kind.data.event_id === eventId),
  );
}

/**
 * A previewed room's picture: none for a room the account is invited to while it hides
 * pictures in invitations, so the preview shows initials and fetches nothing (ADR 0011).
 */
export function roomPreviewPicture(
  state: State,
  id: AccountId,
  preview: RoomPreview | null,
): MediaRef | null {
  if (!preview?.avatar) return null;

  const data = account(state, id);

  const invited =
    preview.invited || (data?.invitations.some((i) => i.room_id === preview.room_id) ?? false);

  return invited && !(data?.safety.invite_pictures ?? true) ? null : preview.avatar;
}

/**
 * The picture a gallery thumbnail fetches: none for media the account keeps back, so a hidden
 * item shows its kind's icon until the user taps Show.
 */
export function galleryThumbnail(
  state: State,
  id: AccountId,
  roomId: string,
  message: Message,
): MediaRef | null {
  const attachment = message.content.type === 'Attachment' ? message.content.data : null;

  if (!attachment || previewHidden(state, id, roomId, message)) return null;

  return thumbnailOf(attachment)?.media ?? null;
}
