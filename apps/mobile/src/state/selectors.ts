// Derived data views read: kept here so every screen derives it the same way.

import type {
  AccountId,
  MediaRef,
  Message,
  MediaSize,
  RoomPreview,
  SavedMessage,
  SharedFile,
  TimelineEntry,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { galleryAround, thumbnailOf } from './gallery';
import { EMPTY_TIMELINE, mediaKey } from './reducer';
import { ignores, revealKey, showsMedia, shownInvitations } from './safety';
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
  const data = activeAccount(state);

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

/** The people shown as ignored whose ignore the engine hasn't confirmed yet. */
export function pendingIgnored(data: AccountData): string[] {
  return data.safety.ignored.filter((id) => !data.savedSafety.ignored.includes(id));
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
      ? { id: entry.id, kind: { type: 'Other' } }
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

  return data.saved.filter((m) => saved.has(`${m.room_id}|${m.preview.event_id}`));
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
  const publicRoom = data.rooms.find((r) => r.room_id === roomId)?.public ?? false;

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
 * The attachment preview's gallery around `current`: the batch, without media the account keeps
 * back. The opened message stays, so the preview has its item, and shows hidden until the user
 * taps Show (`previewHidden`), whichever screen opened it (ADR 0011). A file Room Info listed
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

    return file &&
      data &&
      !ignores(data.safety, file.sender_id) &&
      !heldAnywhere(data, key.room_id, opened)
      ? [listedMessage(file)]
      : [];
  }

  return found.filter((m) => m.event_id === opened || !previewHidden(state, id, key.room_id, m));
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

/** Each room's listed files as Room Info last showed them, by the list and ignore list. */
const shownFileLists = new WeakMap<SharedFile[], { ignored: string[]; shown: SharedFile[] }>();

/**
 * The files Room Info lists for a room: without those people the user ignores shared (as
 * shown, so a pending ignore hides them too). The same array while nothing changes.
 */
export function shownFiles(state: State, id: AccountId, roomId: string): SharedFile[] | undefined {
  const data = account(state, id);
  const files = data?.files[roomId];

  if (!data || !files || data.safety.ignored.length === 0) return files;
  const ignored = data.safety.ignored;
  const cached = shownFileLists.get(files);

  if (cached?.ignored === ignored) return cached.shown;
  const shown = files.filter((file) => !ignored.includes(file.sender_id));
  shownFileLists.set(files, { ignored, shown });

  return shown;
}

/** Whether any of a room's kept timelines holds `eventId`, or held it from an ignored sender. */
function heldAnywhere(data: AccountData, roomId: string, eventId: string): boolean {
  if (data.hiddenEvents[eventId]) return true;

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
