// The reducer: the one function that folds engine events into the app's state, as the
// desktop's `State::apply` does. Pure: the same state and event always give the same result,
// so every behaviour is testable by feeding events (state/__tests__).

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  AccountId,
  AccountSummary,
  MediaRef,
  MediaSize,
  Safety,
  SidebarLayout,
  TimelineDiff,
  TimelineEntry,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { t } from '../i18n';
import { initialCallView, reconcileView } from './callView';
import { keepUnchanged } from './json';
import { DEFAULT_SAFETY, hiddenEntry, withChange } from './safety';
import { failureReason, newlyFailed } from './schedule';
import { DEFAULT_SETTINGS, type Settings } from './settings';
import { applyDiffs, timelineId } from './timeline';
import type { AccountData, PendingSafety, State, Timeline, Verification } from './types';

/** A fetched rendition's key in `State.media`. */
export function mediaKey(media: MediaRef, size: MediaSize): string {
  return size.type === 'Original'
    ? `${media}|original`
    : `${media}|${size.data.width}x${size.data.height}`;
}

/** An edited message's key in `AccountData.edits`. */
export function editsKey(roomId: string, eventId: string): string {
  return `${roomId}|${eventId}`;
}

/** A link card's key in `State.linkPreviews`: previews are fetched per account. */
export function previewKey(account: AccountId, url: string): string {
  return `${account}|${url}`;
}

export function initialState(settings: Settings = DEFAULT_SETTINGS): State {
  return {
    accounts: [],
    active: null,
    restoring: [],
    started: false,
    ended: [],
    media: {},
    gifs: {},
    revealedMedia: {},
    linkPreviews: {},
    call: {
      current: null,
      ended: null,
      levels: {},
      rings: [],
      devices: null,
      mediaError: null,
      view: initialCallView(),
    },
    verifications: [],
    recoveryKey: null,
    notices: [],
    composers: {},
    recentEmoji: [],
    settings,
    giphyKey: null,
    pushers: {},
  };
}

function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  if (!(key in record)) return record;
  const { [key]: _gone, ...rest } = record;

  return rest;
}

const EMPTY_LAYOUT: SidebarLayout = { scopes: {}, changed_at: 0 };

function newAccount(summary: AccountSummary): AccountData {
  return {
    summary,
    rooms: [],
    spaces: [],
    invitations: [],
    roomsLoaded: false,
    scope: 'home',
    timelines: {},
    latest: {},
    heardAt: {},
    typing: {},
    security: null,
    identities: {},
    sessions: null,
    contacts: null,
    members: {},
    roomSettings: {},
    threads: {},
    files: {},
    widgets: {},
    pins: {},
    saved: null,
    packs: {},
    packsMayAdd: {},
    scheduled: null,
    edits: {},
    sidebar: EMPTY_LAYOUT,
    searchUsage: { rooms: {}, people: {} },
    sidebarLoaded: false,
    status: 'Online',
    savedStatus: 'Online',
    keywords: null,
    safety: DEFAULT_SAFETY,
    savedSafety: DEFAULT_SAFETY,
    pendingSafety: [],
    hiddenEvents: {},
  };
}

export const EMPTY_TIMELINE: Timeline = {
  entries: [],
  readBoundary: { kind: 'unknown' },
  reachedStart: false,
  loadingOlder: false,
  historyError: null,
  opening: false,
  openFailed: false,
};

/**
 * `state` with one account changed by `change`; unchanged when the account isn't signed in or
 * `change` returned it as it was.
 */
export function updateAccount(
  state: State,
  id: AccountId,
  change: (account: AccountData) => AccountData,
): State {
  const index = state.accounts.findIndex((a) => a.summary.id === id);

  const before = state.accounts[index];

  if (before === undefined) return state;
  const after = change(before);

  if (after === before) return state;
  const accounts = state.accounts.slice();
  accounts[index] = after;

  return { ...state, accounts };
}

/**
 * `account` without what it fetched about one room for screens over it (its members, threads,
 * files and pins); `account` itself when it holds none of them. The screens ask afresh.
 */
export function forgetRoomDetails(account: AccountData, roomId: string): AccountData {
  const members = withoutKey(account.members, roomId);
  const threads = withoutKey(account.threads, roomId);
  const files = withoutKey(account.files, roomId);
  const pins = withoutKey(account.pins, roomId);

  if (
    members === account.members &&
    threads === account.threads &&
    files === account.files &&
    pins === account.pins
  )
    return account;

  return { ...account, members, threads, files, pins };
}

/** `state` with one timeline changed by `change`. */
export function updateTimeline(
  state: State,
  account: AccountId,
  key: TimelineKey,
  change: (timeline: Timeline) => Timeline,
): State {
  const id = timelineId(key);

  return updateAccount(state, account, (a) => ({
    ...a,
    timelines: { ...a.timelines, [id]: change(a.timelines[id] ?? EMPTY_TIMELINE) },
  }));
}

function upsertSummary(state: State, summary: AccountSummary): State {
  const exists = state.accounts.some((a) => a.summary.id === summary.id);

  const accounts = exists
    ? state.accounts.map((a) => (a.summary.id === summary.id ? { ...a, summary } : a))
    : [...state.accounts, newAccount(summary)];

  return { ...state, accounts };
}

let nextNotice = 1;

/** `state` with a notice to show once. */
export function addNotice(state: State, message: string, account: AccountId | null = null): State {
  return { ...state, notices: [...state.notices, { id: nextNotice++, message, account }] };
}

/** Whether `reported` gives each participant the level `levels` holds, and no one else. */
function sameLevels(levels: Record<string, number>, reported: Array<[string, number]>): boolean {
  return (
    Object.keys(levels).length === reported.length &&
    reported.every(([key, level]) => levels[key] === level)
  );
}

/** A room's newest message from sync. */
export type LatestMessageEvent = Extract<EngineEvent, { type: 'LatestMessage' }>;

/**
 * Folds `LatestMessage` events into the state, as `apply` folds them one by one; each account's
 * maps are copied once, not once per event, since a first sync reports every room.
 */
export function applyLatestMessages(
  state: State,
  events: readonly LatestMessageEvent[],
  now: number = Date.now(),
): State {
  const changes = new Map<AccountId, Pick<AccountData, 'latest' | 'heardAt'>>();

  for (const { data } of events) {
    const account = state.accounts.find((a) => a.summary.id === data.account);

    // Someone the user ignores says nothing in the room list, whichever came first.
    if (!account || fromIgnored(account, data.message)) continue;
    let maps = changes.get(data.account);

    if (!maps) {
      maps = { latest: { ...account.latest }, heardAt: account.heardAt };
      changes.set(data.account, maps);
    }

    if (maps.latest[data.room_id]?.event_id !== data.message.event_id) {
      if (maps.heardAt === account.heardAt) maps.heardAt = { ...account.heardAt };
      maps.heardAt[data.room_id] = now;
    }

    maps.latest[data.room_id] = data.message;
  }

  let next = state;

  for (const [id, maps] of changes) next = updateAccount(next, id, (a) => ({ ...a, ...maps }));

  return next;
}

/**
 * `account` with the reported choices `saved` and the changes still on their way, shown as
 * `safety`. Whenever the confirmed ignore list gains anyone, what anyone on it said goes
 * (`withoutIgnored`), so the order the answers come in can't keep it.
 */
export function withSafety(
  account: AccountData,
  saved: Safety,
  pending: PendingSafety[],
): AccountData {
  const safety = pending.reduce((shown, p) => withChange(shown, p.change), saved);
  const next = { ...account, safety, savedSafety: saved, pendingSafety: pending };
  // Only a confirmed ignore removes anything: one still on its way hides at read time
  // (`selectors.pendingIgnored`), so one that fails gives everything back.
  const gained = saved.ignored.some((id) => !account.savedSafety.ignored.includes(id));

  return gained ? withoutIgnored(next, new Set(saved.ignored)) : next;
}

/** `state` with a change to `account`'s choices shown at once, until the engine answers. */
export function addPendingSafety(state: State, account: AccountId, pending: PendingSafety): State {
  return updateAccount(state, account, (a) =>
    withSafety(a, a.savedSafety, [...a.pendingSafety, pending]),
  );
}

/**
 * `state` once the engine answered the change `id`: it no longer shows on top of what was
 * reported. A change saved came with its own report; one refused leaves the reported choices and
 * the other changes still on their way, never an older snapshot.
 */
export function settlePendingSafety(state: State, account: AccountId, id: number): State {
  return updateAccount(state, account, (a) =>
    withSafety(
      a,
      a.savedSafety,
      a.pendingSafety.filter((p) => p.id !== id),
    ),
  );
}

/** Whether `message` was sent by someone `account` ignores, as the engine confirmed. */
function fromIgnored(account: AccountData, message: { sender_id: string }): boolean {
  return account.savedSafety.ignored.includes(message.sender_id);
}

/**
 * `entry` drawn as nothing when someone the user ignores sent it, keeping its place; its event
 * ID goes into `hidden`, so nothing shows it again in another form (`AccountData.hiddenEvents`).
 */
function hiddenIfIgnored(
  ignored: ReadonlySet<string>,
  entry: TimelineEntry,
  hidden: Array<[string, string]>,
): TimelineEntry {
  if (entry.kind.type !== 'Message' || !ignored.has(entry.kind.data.sender_id)) return entry;
  hidden.push([entry.kind.data.event_id, entry.kind.data.sender_id]);

  return hiddenEntry(entry.id, entry.kind.data);
}

/** `hiddenEvents` with `events` (event ID, sender) added; the same object when there are none. */
function withHidden(
  hiddenEvents: AccountData['hiddenEvents'],
  events: Array<[string, string]>,
): AccountData['hiddenEvents'] {
  if (events.length === 0) return hiddenEvents;
  const next = { ...hiddenEvents };

  for (const [id, sender] of events) next[id] = sender;

  return next;
}

/** The event IDs `diffs` bring as deleted (redacted) messages. */
function redactedIn(diffs: TimelineDiff[]): string[] {
  const entries = diffs.flatMap((diff): TimelineEntry[] => {
    switch (diff.type) {
      case 'Reset':
      case 'Append':
        return diff.data;
      case 'PushFront':
      case 'PushBack':
        return [diff.data];
      case 'Insert':
      case 'Set':
        return [diff.data.entry];
      case 'Clear':
      case 'PopFront':
      case 'PopBack':
      case 'Remove':
      case 'Truncate':
        return [];
    }
  });

  return entries.flatMap((e) =>
    e.kind.type === 'Message' && e.kind.data.content.type === 'Redacted'
      ? [e.kind.data.event_id]
      : [],
  );
}

/**
 * `account` with deleted messages gone from the room's listed files, so Room Info can't open
 * them again once no timeline holds them any more.
 */
function withoutRedactedFiles(
  account: AccountData,
  roomId: string,
  redacted: string[],
): AccountData {
  const files = account.files[roomId];

  if (!files || !files.some((f) => redacted.includes(f.event_id))) return account;

  return {
    ...account,
    files: { ...account.files, [roomId]: files.filter((f) => !redacted.includes(f.event_id)) },
  };
}

/** `diffs` with what ignored people sent drawn as nothing, the positions kept. */
function diffsWithoutIgnored(
  diffs: TimelineDiff[],
  ignored: ReadonlySet<string>,
  hidden: Array<[string, string]>,
): TimelineDiff[] {
  if (ignored.size === 0) return diffs;
  const hide = (entry: TimelineEntry) => hiddenIfIgnored(ignored, entry, hidden);

  return diffs.map((diff): TimelineDiff => {
    switch (diff.type) {
      case 'Reset':
      case 'Append':
        return { ...diff, data: diff.data.map(hide) };
      case 'PushFront':
      case 'PushBack':
        return { ...diff, data: hide(diff.data) };
      case 'Insert':
      case 'Set':
        return { ...diff, data: { ...diff.data, entry: hide(diff.data.entry) } };
      case 'Clear':
      case 'PopFront':
      case 'PopBack':
      case 'Remove':
      case 'Truncate':
        return diff;
    }
  });
}

/**
 * `account` without what the people in `ignored` said: their messages leave the room list's
 * previews (`latest`) and every kept timeline, cold ones included, which no engine reset reaches
 * (ADR 0011). A timeline keeps its length, as the engine's diffs count by position: their
 * entries become hidden events (`hiddenEntry`), which draw nothing, until the engine sends the timeline again.
 */
function withoutIgnored(account: AccountData, ignored: ReadonlySet<string>): AccountData {
  const latest = Object.fromEntries(
    Object.entries(account.latest).filter(([, message]) => !ignored.has(message.sender_id)),
  );

  const timelines: Record<string, Timeline> = {};
  const hidden: Array<[string, string]> = [];

  for (const [id, timeline] of Object.entries(account.timelines)) {
    const hit = timeline.entries.some(
      (e) => e.kind.type === 'Message' && ignored.has(e.kind.data.sender_id),
    );

    timelines[id] = hit
      ? { ...timeline, entries: timeline.entries.map((e) => hiddenIfIgnored(ignored, e, hidden)) }
      : timeline;
  }

  return { ...account, latest, timelines, hiddenEvents: withHidden(account.hiddenEvents, hidden) };
}

/** Folds one event into the state. `now` is when it arrived, in ms since the epoch. */
export function apply(state: State, event: EngineEvent, now: number = Date.now()): State {
  switch (event.type) {
    case 'Restoring':
      return { ...state, restoring: event.data, started: true };
    case 'AccountAdded': {
      const id = event.data.id;
      const next = upsertSummary(state, event.data);
      const wanted = state.settings.lastAccount;
      const active = state.active === null || id === wanted ? id : state.active;

      return {
        ...next,
        active,
        restoring: state.restoring.filter((r) => r !== id),
        ended: state.ended.filter((e) => e.account !== id),
      };
    }

    case 'AccountChanged':
      return upsertSummary(state, event.data);
    case 'AccountRemoved': {
      const accounts = state.accounts.filter((a) => a.summary.id !== event.data);
      const active = state.active === event.data ? (accounts[0]?.summary.id ?? null) : state.active;

      return {
        ...state,
        accounts,
        active,
        restoring: state.restoring.filter((r) => r !== event.data),
        verifications: state.verifications.filter((v) => v.account !== event.data),
        // Its pusher went with its session (signing out removes it first, ADR 0010).
        pushers: withoutKey(state.pushers, event.data),
        // Its drafts go with it; signing in again reads the ones its store kept.
        composers: Object.fromEntries(
          Object.entries(state.composers).filter(([id]) => !id.startsWith(`${event.data}|`)),
        ),
      };
    }

    case 'SessionEnded': {
      const { account, kept } = event.data;

      return {
        ...state,
        restoring: state.restoring.filter((r) => r !== account),
        ended: [...state.ended.filter((e) => e.account !== account), { account, kept }],
      };
    }

    case 'RoomsUpdated':
      return updateAccount(state, event.data.account, (a) => {
        const scope = a.scope;
        // Every change to any room lists them all again: rooms that didn't change keep their
        // objects, so only their rows redraw.
        const rooms = keepUnchanged(a.rooms, event.data.list.rooms);
        const spaces = keepUnchanged(a.spaces, event.data.list.spaces);
        const invitations = keepUnchanged(a.invitations, event.data.list.invitations);

        // A space the user left takes its scope with it.
        const nextScope =
          scope !== 'home' && !spaces.some((s) => s.room_id === scope.space) ? 'home' : scope;

        if (
          a.roomsLoaded &&
          rooms === a.rooms &&
          spaces === a.spaces &&
          invitations === a.invitations &&
          nextScope === scope
        )
          return a;

        return { ...a, rooms, spaces, invitations, roomsLoaded: true, scope: nextScope };
      });
    case 'LatestMessage':
      return applyLatestMessages(state, [event], now);
    case 'TimelineChanged': {
      const owner = state.accounts.find((a) => a.summary.id === event.data.account);
      const ignored = new Set(owner?.savedSafety.ignored);
      const hidden: Array<[string, string]> = [];
      const diffs = diffsWithoutIgnored(event.data.diffs, ignored, hidden);

      const next = updateTimeline(state, event.data.account, event.data.key, (t) => ({
        ...t,
        entries: applyDiffs(t.entries, diffs),
      }));

      const redacted = redactedIn(event.data.diffs);

      return hidden.length === 0 && redacted.length === 0
        ? next
        : updateAccount(next, event.data.account, (a) =>
            withoutRedactedFiles(
              { ...a, hiddenEvents: withHidden(a.hiddenEvents, hidden) },
              event.data.key.room_id,
              redacted,
            ),
          );
    }

    case 'HistoryLoaded':
      return updateTimeline(state, event.data.account, event.data.key, (t) => ({
        ...t,
        loadingOlder: false,
        reachedStart: event.data.reached_start,
        historyError: null,
      }));
    case 'HistoryFailed':
      return updateTimeline(state, event.data.account, event.data.key, (t) => ({
        ...t,
        loadingOlder: false,
        historyError: event.data.error,
      }));
    case 'Typing':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        typing: { ...a.typing, [event.data.room_id]: event.data.people },
      }));
    case 'SecurityChanged':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        security: event.data.security,
      }));
    case 'Sessions':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        sessions: event.data.sessions,
      }));
    case 'ContactDetails':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        contacts: event.data.contacts,
      }));
    case 'Members':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        members: { ...a.members, [event.data.room_id]: event.data.members },
      }));
    case 'RoomSettings':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        roomSettings: { ...a.roomSettings, [event.data.settings.room_id]: event.data.settings },
      }));
    case 'MembersInvalidated':
      // Sync changed who is in the room or their rights: the next look lists them again.
      return updateAccount(state, event.data.account, (a) => {
        if (!(event.data.room_id in a.members)) return a;
        const { [event.data.room_id]: _stale, ...members } = a.members;

        return { ...a, members };
      });
    case 'ReadBoundary': {
      const { event_id: eventId, loaded } = event.data;

      const readBoundary: Timeline['readBoundary'] =
        eventId === null
          ? { kind: 'absent' }
          : loaded
            ? { kind: 'loaded', eventId }
            : { kind: 'unresolved', eventId };

      return updateTimeline(
        state,
        event.data.account,
        { room_id: event.data.room_id, thread: null },
        (t) => ({ ...t, readBoundary }),
      );
    }

    case 'Threads':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        threads: { ...a.threads, [event.data.room_id]: event.data.threads },
      }));
    case 'PinnedMessages':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        pins: { ...a.pins, [event.data.room_id]: event.data.messages },
      }));
    case 'SavedMessages':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        saved: event.data.messages,
      }));
    case 'Files':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        files: { ...a.files, [event.data.room_id]: event.data.files },
      }));
    case 'Widgets':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        widgets: { ...a.widgets, [event.data.room_id]: event.data.widgets },
      }));
    case 'SidebarLayout':
      return updateAccount(state, event.data.account, (a) =>
        // Of two layouts the later wins, so a sync with an older save never undoes a newer one.
        a.sidebarLoaded && event.data.layout.changed_at < a.sidebar.changed_at
          ? a
          : { ...a, sidebar: event.data.layout, sidebarLoaded: true },
      );
    case 'Keywords':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        keywords: event.data.keywords,
      }));
    case 'Status':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        status: event.data.status,
        savedStatus: event.data.status,
      }));
    case 'Safety':
      return updateAccount(state, event.data.account, (a) =>
        withSafety(a, event.data.safety, a.pendingSafety),
      );
    case 'RecoveryKey':
      return { ...state, recoveryKey: { account: event.data.account, key: event.data.key } };
    case 'Verification': {
      const { account, flow_id: flowId, step } = event.data;

      const same = (v: Verification) => v.account === account && v.flowId === flowId;
      const others = state.verifications.filter((v) => !same(v));

      // Only a request names the person; the later steps keep who it was.
      const user =
        step.type === 'Incoming' ? step.data.user : (state.verifications.find(same)?.user ?? null);

      return { ...state, verifications: [...others, { account, flowId, step, user }] };
    }

    case 'Identity':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        identities: { ...a.identities, [event.data.user_id]: event.data.identity },
      }));

    case 'LinkPreview':
      return {
        ...state,
        linkPreviews: {
          ...state.linkPreviews,
          [previewKey(event.data.account, event.data.url)]: event.data.preview,
        },
      };
    case 'MediaFile':
      return {
        ...state,
        media: { ...state.media, [mediaKey(event.data.media, event.data.size)]: event.data.path },
      };
    case 'GifFile':
      return { ...state, gifs: { ...state.gifs, [event.data.url]: event.data.path } };
    case 'Notification':
      // Shown by the system (`engine/notifications.ts`); the app keeps no list of them.
      return state;
    case 'AccountError':
      // A saved account that failed to restore is no longer restoring.
      return addNotice(
        { ...state, restoring: state.restoring.filter((r) => r !== event.data.account) },
        event.data.error,
        event.data.account,
      );
    case 'CallUpdated': {
      // A new call starts with the default view; the same call keeps it, minus what went.
      const same = state.call.current?.call === event.data.call;
      const view = reconcileView(same ? state.call.view : initialCallView(), event.data);

      return { ...state, call: { ...state.call, current: event.data, ended: null, view } };
    }

    case 'CallEnded': {
      const { account, room_id: roomId, call, reason } = event.data;
      const current = state.call.current?.call === call ? null : state.call.current;
      const view = current ? state.call.view : initialCallView();

      return {
        ...state,
        call: {
          ...state.call,
          current,
          levels: current ? state.call.levels : {},
          view,
          ended: { account, roomId, reason },
        },
      };
    }

    case 'CallLevels':
      // They come every 100 ms in a call, mostly as they were: only a change redraws.
      return sameLevels(state.call.levels, event.data)
        ? state
        : { ...state, call: { ...state.call, levels: Object.fromEntries(event.data) } };
    case 'CallMediaFailed':
      return { ...state, call: { ...state.call, mediaError: event.data } };
    case 'IncomingCall': {
      const ring = event.data;

      const rings = state.call.rings.filter(
        (r) => !(r.account === ring.account && r.room_id === ring.room_id),
      );

      return { ...state, call: { ...state.call, rings: [...rings, ring] } };
    }

    case 'RingEnded': {
      const { account, room_id: roomId } = event.data;

      const rings = state.call.rings.filter(
        (r) => !(r.account === account && r.room_id === roomId),
      );

      return { ...state, call: { ...state.call, rings } };
    }

    case 'CallDevices':
      return { ...state, call: { ...state.call, devices: event.data } };
    case 'SearchUsage':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        searchUsage: event.data.usage,
      }));
    case 'ImagePacks':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        packs: { ...a.packs, [event.data.room_id ?? '']: event.data.packs },
        packsMayAdd: { ...a.packsMayAdd, [event.data.room_id ?? '']: event.data.may_add },
      }));
    case 'ScheduledMessages': {
      const before = state.accounts.find((a) => a.summary.id === event.data.account)?.scheduled;
      const failed = newlyFailed(before, event.data.messages);

      const next = updateAccount(state, event.data.account, (a) => ({
        ...a,
        scheduled: event.data.messages,
      }));

      return failed === null
        ? next
        : addNotice(
            next,
            t('mobile.composer.scheduled_lost', { error: failureReason(failed) }),
            event.data.account,
          );
    }

    case 'EditHistory':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        edits: {
          ...a.edits,
          [editsKey(event.data.room_id, event.data.event_id)]: event.data.versions,
        },
      }));
    // Answers to one request: the store hands them to whoever sent it (requests.ts).
    case 'Finished':
    case 'HomeserverDiscovered':
    case 'OpenUrl':
    case 'ContactCodeSent':
    case 'MessagesFound':
    case 'PeopleFound':
    case 'PlacesFound':
    case 'MessageLocated':
    case 'EventSource':
    // A widget's run (`state/actions/widgets.ts`): its page, messages and questions.
    case 'WidgetPage':
    case 'WidgetMessage':
    case 'WidgetPermissions':
    case 'IntegrationTerms':
    // Answers the composer's own read (`state/actions/drafts.ts`).
    case 'DraftLoaded':
    case 'DirectoryFound':
    case 'DirectChat':
    case 'PublicRooms':
    case 'RoomCreated':
    case 'RoomPreview':
    case 'RoomJoined':
    case 'PasswordNeeded':
    case 'QrLogin':
    case 'Registration':
    case 'QrCodeRead':
    case 'KeysImported':
    // Answers the verification sheet's own request (`AskedVerification`).
    case 'VerificationRequested':
    // Names the account a sign-in command added (`SignInFlow`); `AccountAdded` added it.
    case 'SignedIn':
    case 'MediaFailed':
    case 'CallMediaStopped':
    case 'PushContent':
    // Presence dots and status messages are desktop-only for now.
    case 'Presence':
      return state;
    // Answers its search alone (`store.run`), into the picker that asked.
    case 'GifsFound':
      return state;
    // Answers its change alone (`store.run`); the packs are listed again after it.
    case 'ImagePackEdited':
      return state;
    // The bridge reports these as files (MediaFile, GifFile) instead.
    case 'MediaLoaded':
    case 'GifLoaded':
      return state;
    default: {
      // A new event type must be handled above; unknown ones (a newer engine) change nothing.
      const unhandled: never = event;

      return unhandled;
    }
  }
}
