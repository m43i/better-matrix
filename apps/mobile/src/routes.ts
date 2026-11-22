// Every screen's address, built in one place so params are always encoded the same way.
// Account and room IDs contain `@`, `!`, `:` and `$`; expo-router encodes object params.

import type { AccountId, ReportTarget } from '@better-matrix/matrix-native/types';
import type { Href } from 'expo-router';

export const routes = {
  /** The Home tab, under every other screen. */
  home: (): Href => '/',
  signIn: (resume?: AccountId): Href => ({
    pathname: '/sign-in',
    params: resume ? { resume } : {},
  }),
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
  room: (account: AccountId, roomId: string, eventId?: string, localTarget = false): Href => ({
    pathname: '/room/[account]/[roomId]',
    params: messageParams(account, roomId, eventId, localTarget),
  }),
  thread: (
    account: AccountId,
    roomId: string,
    root: string,
    eventId?: string,
    localTarget = false,
  ): Href => ({
    pathname: '/thread/[account]/[roomId]/[root]',
    params: { ...messageParams(account, roomId, eventId, localTarget), root },
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
  newRoom: (space?: string): Href => ({ pathname: '/new-room', params: space ? { space } : {} }),
  newSpace: (): Href => '/new-space',
  renameRoom: (account: AccountId, roomId: string): Href => ({
    pathname: '/rename-room',
    params: { account, roomId },
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
  verify: (account: AccountId, flowId?: string): Href => ({
    pathname: '/verify',
    params: flowId ? { account, flowId } : { account },
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

type MessageParams = {
  account: AccountId;
  roomId: string;
  eventId?: string;
  localTarget?: string;
};

function messageParams(
  account: AccountId,
  roomId: string,
  eventId?: string,
  localTarget = false,
): MessageParams {
  const params: MessageParams = { account, roomId };

  if (eventId) params.eventId = eventId;

  if (localTarget) params.localTarget = '1';

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
