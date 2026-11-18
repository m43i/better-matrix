// Actions every screen shares: the account and scope on screen, timelines, media, settings.

import type {
  AccountId,
  MediaRef,
  MediaSize,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { forgetRoomDetails, mediaKey, previewKey, updateAccount, updateTimeline } from '../reducer';
import type { CommandFailed } from '../requests';
import type { Settings } from '../settings';
import type { Store } from '../store';
import { timelineId } from '../timeline';
import type { Scope, State, Timeline } from '../types';
import { nextSpeed } from '../voiceCard';

/** Puts an account on screen. */
export function setActive(store: Store, account: AccountId): void {
  store.update((s) => ({
    ...s,
    active: account,
    settings: { ...s.settings, lastAccount: account },
  }));
}

/** Shows Home or a space in an account's room list. */
export function setScope(store: Store, account: AccountId, scope: Scope): void {
  store.update((s) => updateAccount(s, account, (a) => ({ ...a, scope })));
}

/** Changes this device's preferences. */
export function updateSettings(store: Store, change: Partial<Settings>): void {
  store.update((s) => ({ ...s, settings: { ...s.settings, ...change } }));
}

/** Steps the playback speed (1×, 1.5×, 2×), for the clip playing and every one after. */
export function cyclePlaybackSpeed(store: Store): void {
  store.update((s) => ({
    ...s,
    settings: { ...s.settings, playbackSpeed: nextSpeed(s.settings.playbackSpeed) },
  }));
}

/**
 * Starts following a timeline: its entries arrive as `TimelineChanged`. One that fails to open
 * shows why where its history would load, with Try again (`retryHistory`).
 */
export function openTimeline(store: Store, account: AccountId, key: TimelineKey): void {
  const followed = followedOf(store, account);
  followed.warm.delete(timelineId(key));
  followed.open.set(timelineId(key), key.room_id);
  store.update((s) =>
    updateTimeline(s, account, key, (t) => ({
      ...t,
      opening: true,
      openFailed: false,
      historyError: null,
    })),
  );
  store.run({ type: 'OpenTimeline', data: { account, key } }).then(
    () => store.update((s) => updateTimeline(s, account, key, (t) => ({ ...t, opening: false }))),
    (error: CommandFailed) => {
      const failed = (t: Timeline): Timeline =>
        error.cancelled
          ? { ...t, opening: false }
          : {
              ...t,
              opening: false,
              loadingOlder: false,
              historyError: error.message,
              openFailed: true,
            };

      store.update((s) => updateTimeline(s, account, key, failed));
    },
  );
}

/** A room screen is being shown; background subscriptions never record a visit. */
export function visitRoom(store: Store, account: AccountId, roomId: string): void {
  store.send({ type: 'VisitRoom', data: { account, room_id: roomId } }, { quiet: true });
}

/**
 * How many closed timelines an account's engine keeps warm (`WARM_TIMELINES` in matrix-core's
 * engine): opening one of them again gives back what it had.
 */
export const WARM_TIMELINES = 8;

/** The most entries a timeline the engine let go of keeps: one page of history. */
export const COLD_ENTRIES = 30;

/**
 * An account's open timelines, and its closed ones the engine keeps warm (closed longest ago
 * first), each by `timelineId` with its room's ID.
 */
interface Followed {
  open: Map<string, string>;
  warm: Map<string, string>;
}

const followed = new WeakMap<Store, Map<AccountId, Followed>>();

function followedOf(store: Store, account: AccountId): Followed {
  const accounts = followed.get(store) ?? new Map<AccountId, Followed>();
  followed.set(store, accounts);
  const timelines = accounts.get(account) ?? { open: new Map(), warm: new Map() };
  accounts.set(account, timelines);

  return timelines;
}

/** Whether a timeline of `roomId` (its main one or a thread) is open or warm. */
function roomFollowed({ open, warm }: Followed, roomId: string): boolean {
  for (const room of open.values()) if (room === roomId) return true;

  for (const room of warm.values()) if (room === roomId) return true;

  return false;
}

/**
 * Stops following a timeline; the engine keeps it warm, and the store its entries. Once the
 * engine lets it go, closed more than `WARM_TIMELINES` timelines ago, it is a cold timeline: the
 * store keeps only its newest entries (`coldTimeline`), since opening it again starts over from
 * the engine's newest anyway. Once no timeline of its room is open or warm, the room's members,
 * threads, files and pins go too (`forgetRoomDetails`): the screens showing them ask afresh.
 */
export function closeTimeline(store: Store, account: AccountId, key: TimelineKey): void {
  store.send({ type: 'CloseTimeline', data: { account, key } }, { quiet: true });

  const id = timelineId(key);
  const timelines = followedOf(store, account);
  timelines.open.delete(id);
  timelines.warm.delete(id);

  // One that never opened isn't kept warm.
  if (timelineOf(store, account, id)?.openFailed) return;
  timelines.warm.set(id, key.room_id);

  for (const [cold, roomId] of timelines.warm) {
    if (timelines.warm.size <= WARM_TIMELINES) break;
    timelines.warm.delete(cold);
    const forget = !roomFollowed(timelines, roomId);

    store.update((s) =>
      updateAccount(s, account, (a) => {
        const timeline = a.timelines[cold];
        const kept = timeline && coldTimeline(timeline);

        const trimmed =
          kept && kept !== timeline ? { ...a, timelines: { ...a.timelines, [cold]: kept } } : a;

        return forget ? forgetRoomDetails(trimmed, roomId) : trimmed;
      }),
    );
  }
}

/**
 * What the store keeps of a timeline the engine let go of: its newest `COLD_ENTRIES` entries, and
 * at least its newest message, which the room list previews and orders by (`lastMessage`).
 * History before them is gone, so it hasn't reached the start any more.
 */
export function coldTimeline(timeline: Timeline): Timeline {
  const { entries } = timeline;
  let from = entries.length - COLD_ENTRIES;

  if (from <= 0) return timeline;

  for (let i = entries.length - 1; i >= 0; i--) {
    if (entries[i]?.kind.type === 'Message') {
      from = Math.min(from, i);
      break;
    }
  }

  return from <= 0 ? timeline : { ...timeline, entries: entries.slice(from), reachedStart: false };
}

function timelineOf(store: Store, account: AccountId, id: string): Timeline | undefined {
  return store.getState().accounts.find((a) => a.summary.id === account)?.timelines[id];
}

/**
 * Asks for the page of history before what's loaded, unless one is coming, none is left or the
 * timeline is still opening (its first page comes with it). A
 * page that fails ends with `HistoryFailed` or only with the command's failure: either shows the
 * error, with Try again (a cancelled page just stops loading).
 */
export function loadOlder(store: Store, account: AccountId, key: TimelineKey): void {
  const timeline = store.getState().accounts.find((a) => a.summary.id === account)?.timelines[
    timelineId(key)
  ];

  if (timeline?.loadingOlder || timeline?.reachedStart || timeline?.opening) return;
  store.update((s) =>
    updateTimeline(s, account, key, (t) => ({ ...t, loadingOlder: true, historyError: null })),
  );
  store.run({ type: 'LoadOlder', data: { account, key } }).catch((error: CommandFailed) => {
    const historyError = error.message === 'cancelled' ? null : error.message;
    store.update((s) =>
      updateTimeline(s, account, key, (t) => ({ ...t, loadingOlder: false, historyError })),
    );
  });
}

/**
 * Marks a timeline read up to `eventId` (the newest message on screen; `null` for the newest
 * event). The cross-device fully-read marker follows only on a room's main timeline whose read
 * boundary is known and so has been passed (as the desktop's receipts decide), or when the user
 * asks outright (`fullyRead`).
 */
export function markRead(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  eventId: string | null = null,
  fullyRead: boolean = mayMarkFullyRead(store, account, key),
): void {
  store.send(
    { type: 'MarkRead', data: { account, key, event_id: eventId, fully_read: fullyRead } },
    { quiet: true },
  );
}

/** Whether reading to the newest message has passed the room's read boundary. */
export function mayMarkFullyRead(store: Store, account: AccountId, key: TimelineKey): boolean {
  if (key.thread !== null) return false;

  const boundary = store.getState().accounts.find((a) => a.summary.id === account)?.timelines[
    timelineId(key)
  ]?.readBoundary;

  return boundary?.kind === 'absent' || boundary?.kind === 'loaded';
}

/**
 * Wants media fetched while a view shows it (`Store.want`); the file shows up in `State.media`
 * (see `useMedia`). The returned function says the view no longer wants it.
 */
export function wantMedia(
  store: Store,
  account: AccountId,
  media: MediaRef,
  size: MediaSize,
): () => void {
  return store.want(`media:${mediaKey(media, size)}`, {
    type: 'FetchMedia',
    data: { account, media, size },
  });
}

/** Wants a link's card fetched while a view shows it (`Store.want`). */
export function wantLinkPreview(store: Store, account: AccountId, url: string): () => void {
  return store.want(`preview:${previewKey(account, url)}`, {
    type: 'PreviewLink',
    data: { account, url },
  });
}

/** Wants a linked GIF fetched while a view shows it (`Store.want`). */
export function wantGif(store: Store, url: string): () => void {
  return store.want(`gif:${url}`, { type: 'FetchGif', data: { url } });
}

/** Forgets a notice once it was shown. */
export function dismissNotice(store: Store, id: number): void {
  store.update((s): State => ({ ...s, notices: s.notices.filter((n) => n.id !== id) }));
}
