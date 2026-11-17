// Actions every screen shares: the account and scope on screen, timelines, media, settings.

import type {
  AccountId,
  MediaRef,
  MediaSize,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { mediaKey, previewKey, updateAccount, updateTimeline } from '../reducer';
import type { CommandFailed } from '../requests';
import type { Settings } from '../settings';
import type { Store } from '../store';
import { timelineId } from '../timeline';
import type { Scope, State, Timeline } from '../types';

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

/**
 * Starts following a timeline: its entries arrive as `TimelineChanged`. One that fails to open
 * shows why where its history would load, with Try again (`retryHistory`).
 */
export function openTimeline(store: Store, account: AccountId, key: TimelineKey): void {
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

/** Stops following a timeline; the engine keeps it warm, the store keeps its entries. */
export function closeTimeline(store: Store, account: AccountId, key: TimelineKey): void {
  store.send({ type: 'CloseTimeline', data: { account, key } }, { quiet: true });
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

/** Fetches media once; the file shows up in `State.media` (see `useMedia`). */
export function fetchMedia(
  store: Store,
  account: AccountId,
  media: MediaRef,
  size: MediaSize,
): void {
  store.fetchOnce(`media:${mediaKey(media, size)}`, {
    type: 'FetchMedia',
    data: { account, media, size },
  });
}

/** Fetches a link's card once. */
export function fetchLinkPreview(store: Store, account: AccountId, url: string): void {
  store.fetchOnce(`preview:${previewKey(account, url)}`, {
    type: 'PreviewLink',
    data: { account, url },
  });
}

/** Fetches a linked GIF once. */
export function fetchGif(store: Store, url: string): void {
  store.fetchOnce(`gif:${url}`, { type: 'FetchGif', data: { url } });
}

/** Forgets a notice once it was shown. */
export function dismissNotice(store: Store, id: number): void {
  store.update((s): State => ({ ...s, notices: s.notices.filter((n) => n.id !== id) }));
}
