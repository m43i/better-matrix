// Every screen's address, built in one place so params are always encoded the same way.
// Account and room IDs contain `@`, `!`, `:` and `$`; expo-router encodes object params.

import type { AccountId } from '@better-matrix/matrix-native/types';
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
  member: (account: AccountId, roomId: string, userId: string): Href => ({
    pathname: '/member',
    params: { account, roomId, userId },
  }),
  invite: (account: AccountId, roomId: string): Href => ({
    pathname: '/invite',
    params: { account, roomId },
  }),
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
