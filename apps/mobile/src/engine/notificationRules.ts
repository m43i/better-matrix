// What a system notification for `Event::Notification` says and when it shows: pure, so it is
// tested without the native module (notifications.ts presents what this decides).

import { z } from 'zod';
import type { AccountId, Notification, PushContent } from '@better-matrix/matrix-native/types';

import { t } from '../i18n';
import type { State } from '../state/types';

/** Whether to show a notification: not while its room is on screen in the foreground. */
export function shouldPresent(foreground: boolean, roomOnScreen: boolean): boolean {
  return !(foreground && roomOnScreen);
}

/** Where a tapped notification leads, kept in its data. */
export interface NotificationTarget {
  account: AccountId;
  roomId: string;
  eventId: string;
}

export interface NotificationContent {
  /** One notification per event, so a repeat replaces it. */
  identifier: string;
  title: string;
  body: string;
  /** Groups a room's notifications (iOS thread, Android tag). */
  thread: string;
  data: { account: string; roomId: string; eventId: string };
}

/**
 * A one-to-one direct chat names its sender. Group conversations use the room and sender.
 */
export function contentOf(n: Notification): NotificationContent {
  return {
    identifier: `${n.account}|${n.event_id}`,
    title: n.one_to_one ? n.sender_name : n.room_name,
    body:
      n.one_to_one || n.sender_name === n.room_name
        ? n.body
        : t('overview.preview', { sender: n.sender_name, body: n.body }),
    thread: `${n.account}|${n.room_id}`,
    data: { account: n.account, roomId: n.room_id, eventId: n.event_id },
  };
}

/**
 * The notification for what a push turned out to be (engine/push.ts): a message as the sync's,
 * an invite, or a generic text in its room when the event can't be read. Null for nothing.
 */
export function pushContentOf(content: PushContent): NotificationContent | null {
  switch (content.type) {
    case 'Message':
      return contentOf(content.data);
    case 'Invite': {
      const { account, room_id, room_name, event_id, sender_name } = content.data;

      return {
        identifier: `${account}|${event_id}`,
        title: room_name,
        body: t('mobile.push.invite', { sender: sender_name }),
        thread: `${account}|${room_id}`,
        data: { account, roomId: room_id, eventId: event_id },
      };
    }

    case 'Hidden': {
      const { account, room_id, room_name, event_id } = content.data;

      return {
        identifier: `${account}|${event_id}`,
        title: room_name ?? t('app.name'),
        body: t('mobile.push.new_message'),
        thread: `${account}|${room_id}`,
        data: { account, roomId: room_id, eventId: event_id },
      };
    }

    case 'Nothing':
      return null;
  }
}

/**
 * The room a tapped notification opens, if its data is one of ours: the app's own
 * (`roomId`, `eventId`), or a push's as the gateway sent it (`room_id`, `event_id`, which iOS
 * keeps when the Notification Service Extension rewrote the text).
 */
export const targetOf = z
  .object({
    account: z.string(),
    roomId: z.json().optional(),
    room_id: z.json().optional(),
    eventId: z.json().optional(),
    event_id: z.json().optional(),
  })
  .transform((fields) => ({
    account: fields.account,
    roomId: fields.roomId ?? fields.room_id,
    eventId: fields.eventId ?? fields.event_id,
  }))
  .pipe(z.object({ account: z.string(), roomId: z.string(), eventId: z.string() }))
  .nullable()
  .catch(null).parse;

/**
 * Where a tapped notification goes: the invitation's sheet when it is about a pending invitation
 * (an invited room isn't a conversation yet), the room when the account has joined it. `wait`
 * while the account's lists know neither: the room list from before the first sync may not have
 * a room joined or an invitation received while the app was closed. Once `waitedOut`, the room.
 */
export function tapRoute(
  state: State,
  target: NotificationTarget,
  waitedOut = false,
): 'wait' | 'invitation' | 'room' {
  const account = state.accounts.find((a) => a.summary.id === target.account);

  if (account?.invitations.some((invite) => invite.room_id === target.roomId)) return 'invitation';

  const joined =
    account?.rooms.some((room) => room.room_id === target.roomId) ||
    account?.spaces.some((space) => space.room_id === target.roomId);

  return joined || waitedOut ? 'room' : 'wait';
}
