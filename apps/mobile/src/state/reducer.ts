// The reducer: the one function that folds engine events into the app's state, as the
// desktop's `State::apply` does. Pure: the same state and event always give the same result,
// so every behaviour is testable by feeding events (state/__tests__).

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  AccountId,
  AccountSummary,
  MediaRef,
  MediaSize,
  SidebarLayout,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { initialCallView, reconcileView } from './callView';
import { DEFAULT_SETTINGS, type Settings } from './settings';
import { applyDiffs, timelineId } from './timeline';
import type { AccountData, State, Timeline } from './types';

/** A fetched rendition's key in `State.media`. */
export function mediaKey(media: MediaRef, size: MediaSize): string {
  return size.type === 'Original'
    ? `${media}|original`
    : `${media}|${size.data.width}x${size.data.height}`;
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
    sessions: null,
    contacts: null,
    members: {},
    threads: {},
    files: {},
    pins: {},
    sidebar: EMPTY_LAYOUT,
    searchUsage: { rooms: {}, people: {} },
    sidebarLoaded: false,
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

/** `state` with one account changed by `change`; unchanged when the account isn't signed in. */
export function updateAccount(
  state: State,
  id: AccountId,
  change: (account: AccountData) => AccountData,
): State {
  const index = state.accounts.findIndex((a) => a.summary.id === id);

  if (index < 0) return state;
  const accounts = state.accounts.slice();
  accounts[index] = change(accounts[index]!);

  return { ...state, accounts };
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

        return {
          ...a,
          rooms: event.data.list.rooms,
          spaces: event.data.list.spaces,
          invitations: event.data.list.invitations,
          roomsLoaded: true,
          // A space the user left takes its scope with it.
          scope:
            scope !== 'home' && !event.data.list.spaces.some((s) => s.room_id === scope.space)
              ? 'home'
              : scope,
        };
      });
    case 'LatestMessage':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        latest: { ...a.latest, [event.data.room_id]: event.data.message },
        heardAt:
          a.latest[event.data.room_id]?.event_id === event.data.message.event_id
            ? a.heardAt
            : { ...a.heardAt, [event.data.room_id]: now },
      }));
    case 'TimelineChanged':
      return updateTimeline(state, event.data.account, event.data.key, (t) => ({
        ...t,
        entries: applyDiffs(t.entries, event.data.diffs),
      }));
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
    case 'Files':
      return updateAccount(state, event.data.account, (a) => ({
        ...a,
        files: { ...a.files, [event.data.room_id]: event.data.files },
      }));
    case 'SidebarLayout':
      return updateAccount(state, event.data.account, (a) =>
        // Of two layouts the later wins, so a sync with an older save never undoes a newer one.
        a.sidebarLoaded && event.data.layout.changed_at < a.sidebar.changed_at
          ? a
          : { ...a, sidebar: event.data.layout, sidebarLoaded: true },
      );
    case 'RecoveryKey':
      return { ...state, recoveryKey: { account: event.data.account, key: event.data.key } };
    case 'Verification': {
      const { account, flow_id: flowId, step } = event.data;

      const others = state.verifications.filter(
        (v) => !(v.account === account && v.flowId === flowId),
      );

      return { ...state, verifications: [...others, { account, flowId, step }] };
    }

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
      return { ...state, call: { ...state.call, levels: Object.fromEntries(event.data) } };
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
    // Answers to one request: the store hands them to whoever sent it (requests.ts).
    case 'Finished':
    case 'HomeserverDiscovered':
    case 'OpenUrl':
    case 'ContactCodeSent':
    case 'MessagesFound':
    case 'PeopleFound':
    case 'DirectChat':
    case 'PublicRooms':
    case 'RoomCreated':
    case 'RoomPreview':
    case 'RoomJoined':
    case 'PasswordNeeded':
    case 'MediaFailed':
    case 'CallMediaStopped':
    case 'PushContent':
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
