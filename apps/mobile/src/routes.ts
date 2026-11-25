// Every screen's address, built in one place so params are always encoded the same way.
// Account and room IDs contain `@`, `!`, `:` and `$`; expo-router encodes object params.

import type { AccountId, JoinRule, ReportTarget } from '@better-matrix/matrix-native/types';
import type { Href } from 'expo-router';

import type { Reach } from './state/seek';
import type { WidgetTarget } from './state/widgets';

export const routes = {
  /** The Home tab, under every other screen. */
  home: (): Href => '/',
  signIn: (resume?: AccountId): Href => ({
    pathname: '/sign-in',
    params: resume ? { resume } : {},
  }),
  /** Signing in by scanning a signed-in device's QR code. */
  signInWithQr: (): Href => ({ pathname: '/sign-in', params: { qr: '1' } }),
  /** Creating an account on `server`, which the sign-in looks up first. */
  register: (server: string): Href => ({ pathname: '/sign-in', params: { server, create: '1' } }),
  /** The account menu, from the avatar in the header: switch or add accounts, settings. */
  accountMenu: (): Href => '/account-menu',
  /** Home and the spaces of the account on screen, from Home's title. */
  spaces: (): Href => '/spaces',
  /** What can be made here (a chat, room, space or section), from the "+" in iOS's tab bar. */
  create: (): Href => '/create',
  roomMenu: (account: AccountId, roomId: string): Href => ({
    pathname: '/room-menu',
    params: { account, roomId },
  }),
  /** A room list section's sheet (collapse, rename, delete); without `section`, naming a new one. */
  sectionMenu: (account: AccountId, section?: string): Href => ({
    pathname: '/room-menu',
    params: section ? { account, section } : { account },
  }),
  /** A room's timeline, opened at `eventId` when given, looked for as far as `reach` says. */
  room: (account: AccountId, roomId: string, eventId?: string, reach: Reach = 'near'): Href => ({
    pathname: '/room/[account]/[roomId]',
    params: messageParams(account, roomId, eventId, reach),
  }),
  thread: (
    account: AccountId,
    roomId: string,
    root: string,
    eventId?: string,
    reach: Reach = 'near',
  ): Href => ({
    pathname: '/thread/[account]/[roomId]/[root]',
    params: { ...messageParams(account, roomId, eventId, reach), root },
  }),
  /** The long-press sheet of one message; `thread` when the message is in a thread's timeline. */
  messageActions: (account: AccountId, roomId: string, eventId: string, thread?: string): Href => ({
    pathname: '/message-actions',
    params: thread ? { account, roomId, eventId, thread } : { account, roomId, eventId },
  }),
  forward: (account: AccountId, roomId: string, eventId: string, thread?: string): Href => ({
    pathname: '/forward-message',
    params: thread ? { account, roomId, eventId, thread } : { account, roomId, eventId },
  }),
  pins: (account: AccountId, roomId: string): Href => ({
    pathname: '/pinned-messages',
    params: { account, roomId },
  }),
  /** An event's source (developer tools). */
  eventSource: (account: AccountId, roomId: string, eventId: string): Href => ({
    pathname: '/event-source',
    params: { account, roomId, eventId },
  }),
  /** A room's widgets: web apps pinned to it, opened in the app or the browser. */
  widgets: (account: AccountId, roomId: string): Href => ({
    pathname: '/widgets',
    params: { account, roomId },
  }),
  /**
   * A widget, a new meeting or the integration manager, running in the app's web view, with the
   * app's own one-time `launch` (`Launches`): a link alone opens nothing.
   */
  widget: (account: AccountId, roomId: string, target: WidgetTarget, launch: string): Href => ({
    pathname: '/widget',
    params: { account, roomId, ...widgetParams(target), launch },
  }),
  /** Pinning a web page to a room as a widget. */
  addWidget: (account: AccountId, roomId: string): Href => ({
    pathname: '/add-widget',
    params: { account, roomId },
  }),
  /** The messages the account saved, from the account menu. */
  saved: (account: AccountId): Href => ({ pathname: '/saved-messages', params: { account } }),
  /** File actions over an attachment preview. */
  attachmentActions: (
    account: AccountId,
    roomId: string,
    eventId: string,
    thread?: string,
  ): Href => ({
    pathname: '/attachment-actions',
    params: thread ? { account, roomId, eventId, thread } : { account, roomId, eventId },
  }),
  /** Full-screen attachment preview of a message's attachment. */
  preview: (account: AccountId, roomId: string, eventId: string, thread?: string): Href => ({
    pathname: '/preview',
    params: thread ? { account, roomId, eventId, thread } : { account, roomId, eventId },
  }),
  /** The emoji picker: reacting to a message (`eventId`), or inserting into a composer. */
  emoji: (
    account: AccountId,
    roomId: string,
    options: { eventId?: string; thread?: string } = {},
  ): Href => ({
    pathname: '/emoji',
    params: { account, roomId, ...options },
  }),
  /** A message's earlier versions, from its "edited" label. */
  editHistory: (account: AccountId, roomId: string, eventId: string): Href => ({
    pathname: '/edit-history',
    params: { account, roomId, eventId },
  }),
  /** The sticker picker, sending into a room's or thread's timeline. */
  stickers: (account: AccountId, roomId: string, thread?: string): Href => ({
    pathname: '/stickers',
    params: thread ? { account, roomId, thread } : { account, roomId },
  }),
  /** Sharing a place: the phone's position or one found by name. */
  location: (account: AccountId, roomId: string, thread?: string): Href => ({
    pathname: '/location',
    params: thread ? { account, roomId, thread } : { account, roomId },
  }),
  /** When to send what the composer holds, from a long press on Send. */
  schedule: (account: AccountId, roomId: string, thread?: string): Href => ({
    pathname: '/schedule',
    params: thread ? { account, roomId, thread } : { account, roomId },
  }),
  /** A conversation's scheduled messages, to send now or cancel. */
  scheduled: (account: AccountId, roomId: string): Href => ({
    pathname: '/scheduled',
    params: { account, roomId },
  }),
  newPoll: (account: AccountId, roomId: string, thread?: string): Href => ({
    pathname: '/new-poll',
    params: thread ? { account, roomId, thread } : { account, roomId },
  }),
  /** An invitation's sheet: accept or decline. */
  invitation: (account: AccountId, roomId: string): Href => ({
    pathname: '/invitation',
    params: { account, roomId },
  }),
  roomInfo: (account: AccountId, roomId: string): Href => ({
    pathname: '/room-info/[account]/[roomId]',
    params: { account, roomId },
  }),
  /** Who has read up to a message in a room's timeline. */
  readers: (account: AccountId, roomId: string, eventId: string): Href => ({
    pathname: '/readers',
    params: { account, roomId, eventId },
  }),
  member: (account: AccountId, roomId: string, userId: string): Href => ({
    pathname: '/member',
    params: { account, roomId, userId },
  }),
  invite: (account: AccountId, roomId: string): Href => ({
    pathname: '/invite',
    params: { account, roomId },
  }),
  /**
   * Reporting a message, a room or a person to the homeserver's admins. `name` is the person's or
   * room's name, or a message's sender's; `sender` is a message's sender, whom the user may also
   * ignore.
   */
  report: (account: AccountId, target: ReportTarget, name: string, sender?: string): Href => {
    const params: ReportParams = { account, name, ...reportParams(target) };

    if (sender) params.userId = sender;

    return { pathname: '/report', params };
  },
  newChat: (): Href => '/new-chat',
  /** A new room, in `space` when given, with `join` chosen ahead. */
  newRoom: (space?: string, join?: JoinRule): Href => {
    const params: NewRoomParams = {};

    if (space) params.space = space;

    if (join) params.join = join;

    return { pathname: '/new-room', params };
  },
  newSpace: (join?: JoinRule): Href => ({ pathname: '/new-space', params: join ? { join } : {} }),
  /** A room's or space's settings: only the parts the user may change. */
  roomSettings: (account: AccountId, roomId: string): Href => ({
    pathname: '/room-settings/[account]/[roomId]',
    params: { account, roomId },
  }),
  /** A room's or space's permission levels, from its settings. */
  roomPermissions: (account: AccountId, roomId: string): Href => ({
    pathname: '/room-permissions',
    params: { account, roomId },
  }),
  /** A space's sheet, from a long press in the space picker: settings, its rooms, leaving. */
  spaceMenu: (account: AccountId, space: string): Href => ({
    pathname: '/space-menu',
    params: { account, space },
  }),
  /** A space's rooms in order: moving, removing and adding them. */
  spaceRooms: (account: AccountId, space: string): Href => ({
    pathname: '/space-rooms',
    params: { account, space },
  }),
  /** Leaving a space, choosing which of its rooms to leave too. */
  leaveSpace: (account: AccountId, space: string): Href => ({
    pathname: '/leave-space',
    params: { account, space },
  }),
  settings: (): Href => '/settings',
  settingsNotifications: (): Href => '/settings/notifications',
  settingsAccount: (account: AccountId): Href => ({
    pathname: '/settings/account',
    params: { account },
  }),
  settingsProfile: (account: AccountId): Href => ({
    pathname: '/settings/profile',
    params: { account },
  }),
  settingsSessions: (account: AccountId): Href => ({
    pathname: '/settings/sessions',
    params: { account },
  }),
  /** What the account keeps away: media, strangers' invitations, ignored people (ADR 0011). */
  settingsSafety: (account: AccountId): Href => ({
    pathname: '/settings/safety',
    params: { account },
  }),
  /** Linking a new device with a QR code: showing this session's code, or scanning the new one's. */
  linkDevice: (account: AccountId, scan = false): Href => ({
    pathname: '/link-device',
    params: scan ? { account, scan: '1' } : { account },
  }),
  /** Exporting the account's message keys to a file, protected with a passphrase. */
  settingsExportKeys: (account: AccountId): Href => ({
    pathname: '/settings/export-keys',
    params: { account },
  }),
  /** Changing a password account's password. */
  settingsPassword: (account: AccountId): Href => ({
    pathname: '/settings/password',
    params: { account },
  }),
  settingsInfo: (): Href => '/settings/info',
  /** This device's preferences: language, appearance, link previews and GIFs. */
  settingsPreferences: (): Href => '/settings/preferences',
  /** Resetting the account's identity, when the recovery key and every other session are lost. */
  settingsReset: (account: AccountId): Href => ({
    pathname: '/settings/reset',
    params: { account },
  }),
  /** Setting up recovery (or with `reset` replacing the key), then showing the new key once. */
  settingsRecoveryKey: (account: AccountId, reset = false): Href => ({
    pathname: '/settings/recovery-key',
    params: reset ? { account, reset: '1' } : { account },
  }),
  /** The verification sheet: following `flowId`, else verifying this session. */
  verify: (account: AccountId, flowId?: string): Href => ({
    pathname: '/verify',
    params: flowId ? { account, flowId } : { account },
  }),
  /** Verifying another person, from their profile. */
  verifyUser: (account: AccountId, userId: string): Href => ({
    pathname: '/verify',
    params: { account, userId },
  }),
  recovery: (account: AccountId): Href => ({ pathname: '/recovery', params: { account } }),
  call: (): Href => '/call',
  /**
   * A permalink (matrix.to or `matrix:`) to a room that isn't joined or to a person: the preview
   * with Join, or the person with Start chat, acting for `account` when given.
   */
  link: (uri: string, account?: AccountId): Href => ({
    pathname: '/link',
    params: account ? { uri, account } : { uri },
  }),
};

/** What the widget screen runs, as route params (`widgetTarget` reads them back). */
type WidgetParams = { widgetId: string } | { meeting: 'video' | 'voice' } | { integrations: '1' };

function widgetParams(target: WidgetTarget): WidgetParams {
  switch (target.type) {
    case 'widget':
      return { widgetId: target.widgetId };
    case 'meeting':
      return { meeting: target.video ? 'video' : 'voice' };
    case 'integrations':
      return { integrations: '1' };
  }
}

/** What `routes.widget` put in its params, if they name something to run. */
export function widgetTarget(params: {
  widgetId?: string;
  meeting?: string;
  integrations?: string;
}): WidgetTarget | null {
  if (params.widgetId) return { type: 'widget', widgetId: params.widgetId };

  if (params.meeting === 'video' || params.meeting === 'voice')
    return { type: 'meeting', video: params.meeting === 'video' };

  if (params.integrations === '1') return { type: 'integrations' };

  return null;
}

/** A new room's params: its space and its join rule, chosen ahead. */
type NewRoomParams = { space?: string; join?: JoinRule };

type MessageParams = {
  account: AccountId;
  roomId: string;
  eventId?: string;
  /** How far the timeline looks for `eventId` (`reachParam` reads it back); none when `near`. */
  reach?: Reach;
};

function messageParams(
  account: AccountId,
  roomId: string,
  eventId?: string,
  reach: Reach = 'near',
): MessageParams {
  const params: MessageParams = { account, roomId };

  if (eventId) {
    params.eventId = eventId;

    if (reach !== 'near') params.reach = reach;
  }

  return params;
}

/** The report screen's params: who reports, the name its title shows, and the target. */
type ReportParams = {
  account: AccountId;
  name: string;
  about: 'message' | 'room' | 'person';
  roomId?: string;
  eventId?: string;
  userId?: string;
};

/** A report's target as route params: what it is about, and its IDs. */
function reportParams(target: ReportTarget) {
  switch (target.type) {
    case 'Message':
      return {
        about: 'message' as const,
        roomId: target.data.room_id,
        eventId: target.data.event_id,
      };
    case 'Room':
      return { about: 'room' as const, roomId: target.data.room_id };
    case 'Person':
      return { about: 'person' as const, userId: target.data.user_id };
  }
}

/** The report target `routes.report` put in its params, if they name one. */
export function reportTarget(params: {
  about?: string;
  roomId?: string;
  eventId?: string;
  userId?: string;
}): ReportTarget | null {
  const { about, roomId, eventId, userId } = params;

  if (about === 'message' && roomId && eventId)
    return { type: 'Message', data: { room_id: roomId, event_id: eventId } };

  if (about === 'room' && roomId) return { type: 'Room', data: { room_id: roomId } };

  if (about === 'person' && userId) return { type: 'Person', data: { user_id: userId } };

  return null;
}
