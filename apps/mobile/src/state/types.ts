// The app's state: everything the engine reported, folded by `apply` (reducer.ts), plus what
// the user chose on this device. Plain data, replaced (never mutated) on every change, so React
// can compare by reference.

import type {
  AccountId,
  SearchUsage,
  AccountSummary,
  CallDevices,
  CallEnd,
  CallInfo,
  Contacts,
  IncomingCall,
  Invitation,
  LinkPreview,
  Members,
  Message,
  Person,
  PinnedMessage,
  RoomSummary,
  Security,
  Session,
  SharedFile,
  SidebarLayout,
  SpaceSummary,
  Status,
  ThreadInfo,
  TimelineEntry,
  VerificationStep,
} from '@better-matrix/matrix-native/types';

import type { CallView } from './callView';

import type { ComposerState } from './composer';
import type { Settings } from './settings';

/** What the sidebar lists: Home, or one space by its room ID. */
export type Scope = 'home' | { space: string };

/** One timeline's entries and paging, as the engine's diffs left them. */
/**
 * Where the room's cross-device read boundary stands (`Event::ReadBoundary`): not reported yet,
 * none on the server, an event outside loaded history, or one in it.
 */
export type ReadBoundary =
  | { kind: 'unknown' }
  | { kind: 'absent' }
  | { kind: 'unresolved'; eventId: string }
  | { kind: 'loaded'; eventId: string };

export interface Timeline {
  entries: TimelineEntry[];
  /** The room's read boundary, for a room's main timeline. */
  readBoundary: ReadBoundary;
  /** The start of visible history is loaded: no older page to ask for. */
  reachedStart: boolean;
  /** A page of history was asked for and hasn't come yet. */
  loadingOlder: boolean;
  /** Why the last page of history failed, until the next one is asked for. */
  historyError: string | null;
  /** The engine is opening the timeline: history can't be asked for yet. */
  opening: boolean;
  /** The timeline didn't open (`historyError` says why): Try again opens it again. */
  openFailed: boolean;
}

/** One signed-in account and what it synced. */
export interface AccountData {
  summary: AccountSummary;
  rooms: RoomSummary[];
  spaces: SpaceSummary[];
  /** Rooms and spaces waiting for the account to accept or decline. */
  invitations: Invitation[];
  /** Whether the room list arrived yet (an empty list before it means "loading"). */
  roomsLoaded: boolean;
  scope: Scope;
  /** Timelines by `timelineId`. */
  timelines: Record<string, Timeline>;
  /** Each room's newest message, from sync, for previews. */
  latest: Record<string, Message>;
  /** When this device heard each room's newest message, ms since the epoch. */
  heardAt: Record<string, number>;
  /** Who is typing, by room ID. */
  typing: Record<string, Person[]>;
  security: Security | null;
  sessions: Session[] | null;
  contacts: Contacts | null;
  members: Record<string, Members>;
  threads: Record<string, ThreadInfo[]>;
  files: Record<string, SharedFile[]>;
  pins: Record<string, PinnedMessage[]>;
  sidebar: SidebarLayout;
  searchUsage: SearchUsage;
  sidebarLoaded: boolean;
  /** What others see of the user, as they set it, including a choice still being saved. */
  status: Status;
  /** The status as last saved: what a choice that fails to save falls back to. */
  savedStatus: Status;
}

/** A verification running for one of the accounts. */
export interface Verification {
  account: AccountId;
  flowId: string;
  step: VerificationStep;
}

/** Something to tell the user once, such as a command that failed. */
export interface Notice {
  id: number;
  message: string;
  account: AccountId | null;
}

export interface CallState {
  /** The call under way. */
  current: CallInfo | null;
  /** Why the last call ended, until the user dismisses it. */
  ended: { account: AccountId; roomId: string; reason: CallEnd } | null;
  /** Who is speaking how loudly (0–255), by participant key ("" for the user). */
  levels: Record<string, number>;
  /** Calls ringing for the user. */
  rings: IncomingCall[];
  devices: CallDevices | null;
  /** Why the user's camera or microphone failed. */
  mediaError: string | null;
  /** How the user sees the call under way: layout, pages, self-view, chat (`state/callView`). */
  view: CallView;
}

export interface State {
  /** Signed-in accounts, in the order they were added. */
  accounts: AccountData[];
  /** The account on screen. */
  active: AccountId | null;
  /** Saved accounts still signing back in at launch. */
  restoring: AccountId[];
  /** Whether the engine has reported which accounts it restores (launch is over). */
  started: boolean;
  /** Sessions the server ended: offered to sign in again. `kept` means a soft logout. */
  ended: Array<{ account: AccountId; kept: boolean }>;
  /** Fetched media files, by `mediaKey`. */
  media: Record<string, string>;
  /** Linked GIF files, by URL. */
  gifs: Record<string, string>;
  /** Link cards, by `previewKey`; `null` when the page has none. */
  linkPreviews: Record<string, LinkPreview | null>;
  call: CallState;
  verifications: Verification[];
  /** A recovery key to show once, right after it was made. */
  recoveryKey: { account: AccountId; key: string } | null;
  notices: Notice[];
  /** Each timeline's composer, by `composerId`. */
  composers: Record<string, ComposerState>;
  /** Emoji the user picked lately, newest first: the picker shows them first. */
  recentEmoji: string[];
  settings: Settings;
  /** The pushers this device registered, by account (engine/push.ts keeps them, saved apart). */
  pushers: Record<AccountId, PusherRegistration>;
}

/** What this device registered as an account's pusher: what removing it needs, and what to compare. */
export interface PusherRegistration {
  /** The push token it was registered with. */
  token: string;
  /** The gateway's notify URL. */
  gateway: string;
  /** The language its fallback text is in. */
  lang: string;
}
