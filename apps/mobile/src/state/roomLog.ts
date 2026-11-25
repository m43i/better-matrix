// The room log: every room event a room's main timeline holds, newest first, as room info lists
// it (the desktop's room log). Changes to a room's setup and to people's profiles are log-only:
// timelines leave them out unless the user shows room changes in conversations (ADR 0006).

import type { RoomChange, RoomEvent, TimelineEntry } from '@better-matrix/matrix-native/types';

/**
 * Whether only the room log lists this change by default, as `RoomChange::log_only` has it:
 * profile and room setup changes. Membership, upgrades and live locations stay in the timeline.
 */
export function logOnly(change: RoomChange): boolean {
  switch (change.type) {
    case 'Created':
    case 'DisplayName':
    case 'ProfilePicture':
    case 'RoomName':
    case 'RoomTopic':
    case 'RoomPicture':
    case 'Encrypted':
    case 'HistoryVisibility':
    case 'JoinRules':
    case 'Permissions':
    case 'PinnedMessages':
    case 'Addresses':
    case 'GuestAccess':
    case 'ServerAccess':
    case 'SpaceRooms':
    case 'Spaces':
    case 'ModerationRules':
      return true;
    case 'Joined':
    case 'Left':
    case 'Invited':
    case 'InviteRevoked':
    case 'InviteDeclined':
    case 'Removed':
    case 'Banned':
    case 'Unbanned':
    case 'Knocked':
    case 'Upgraded':
    case 'InvitedByContact':
    case 'LiveLocation':
      return false;
  }
}

/** One line of the room log: a room event and its timeline entry's ID, which keys it. */
export interface LogEntry {
  id: string;
  event: RoomEvent;
}

/**
 * The room log of a timeline's entries: every room event, membership included, newest first,
 * without those people in `ignored` sent.
 */
export function roomLog(entries: readonly TimelineEntry[], ignored: readonly string[]): LogEntry[] {
  const log: LogEntry[] = [];

  for (let i = entries.length - 1; i >= 0; i--) {
    const entry = entries[i];

    if (entry?.kind.type === 'RoomEvent' && !ignored.includes(entry.kind.data.sender.id))
      log.push({ id: entry.id, event: entry.kind.data });
  }

  return log;
}
