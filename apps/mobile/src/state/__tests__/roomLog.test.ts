import type { RoomChange, TimelineEntry } from '@better-matrix/matrix-native/types';

import { logOnly, roomLog } from '../roomLog';
import { ADA, GRACE, messageEntry, person, roomEventEntry } from '../../test/fixtures';

const ada = person(ADA, 'Ada');

const grace = person(GRACE, 'Grace');

describe('logOnly', () => {
  it.each<RoomChange>([
    { type: 'Created' },
    { type: 'DisplayName', data: 'Ada L.' },
    { type: 'ProfilePicture' },
    { type: 'RoomName', data: 'General' },
    { type: 'RoomTopic', data: 'Plans' },
    { type: 'RoomPicture' },
    { type: 'Encrypted' },
    { type: 'HistoryVisibility', data: 'Shared' },
    { type: 'JoinRules' },
    { type: 'Permissions' },
    { type: 'PinnedMessages' },
    { type: 'Addresses' },
    { type: 'GuestAccess' },
    { type: 'ServerAccess' },
    { type: 'SpaceRooms' },
    { type: 'Spaces' },
    { type: 'ModerationRules' },
  ])('should leave $type to the room log', (change) => {
    expect(logOnly(change)).toBe(true);
  });

  it.each<RoomChange>([
    { type: 'Joined' },
    { type: 'Left' },
    { type: 'Invited', data: grace },
    { type: 'InviteRevoked', data: grace },
    { type: 'InviteDeclined' },
    { type: 'Removed', data: grace },
    { type: 'Banned', data: grace },
    { type: 'Unbanned', data: grace },
    { type: 'Knocked' },
    { type: 'Upgraded' },
    { type: 'InvitedByContact', data: 'grace@example.org' },
    { type: 'LiveLocation' },
  ])('should keep $type in the timeline', (change) => {
    expect(logOnly(change)).toBe(false);
  });
});

describe('roomLog', () => {
  const entries: TimelineEntry[] = [
    roomEventEntry('created', ada, { type: 'Created' }, 1),
    messageEntry('m'),
    roomEventEntry('joined', grace, { type: 'Joined' }, 2),
    roomEventEntry('topic', ada, { type: 'RoomTopic', data: 'Plans' }, 3),
  ];

  it('should list every room event, newest first', () => {
    expect(roomLog(entries, []).map((e) => e.id)).toEqual(['topic', 'joined', 'created']);
  });

  it('should leave out what ignored people did', () => {
    expect(roomLog(entries, [GRACE]).map((e) => e.id)).toEqual(['topic', 'created']);
  });
});
