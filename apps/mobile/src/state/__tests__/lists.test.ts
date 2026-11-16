import type { Notification, RoomSummary } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, message, roomSummary, spaceSummary, summary, withContent } from '../../test/fixtures';
import { activityRows, jumpTo, listTime, messageText, previewLine } from '../lists';
import { apply, initialState } from '../reducer';
import type { AccountData, State } from '../types';

const NOW = new Date(2026, 10, 16, 15, 0).getTime();

function signedIn(...ids: string[]): State {
  return ids.reduce((s, id) => apply(s, { type: 'AccountAdded', data: summary(id, id === ADA ? 'Ada' : 'Grace') }), initialState());
}

function accountWith(rooms: RoomSummary[], spaces = [spaceSummary('!s', [], [], 'Design')]): AccountData {
  const state = apply(signedIn(ADA), { type: 'RoomsUpdated', data: { account: ADA, list: { rooms, spaces, invitations: [] } } });
  return state.accounts[0]!;
}

function notification(account: string, event_id: string, room_id = '!a'): Notification {
  return { account, room_id, room_name: 'Room', event_id, sender_name: 'Grace', body: 'hi' };
}

describe('messageText', () => {
  it('a text message should read as one line', () => {
    expect(messageText(message('$m', 'hello\n\nthere'))).toBe('hello there');
  });

  it('an attachment should read as its file name', () => {
    const content = { type: 'Attachment' as const, data: { kind: 'File' as const, name: 'plan.pdf', size: null, media: null, thumbnail: null, dimensions: null } };
    expect(messageText(withContent('$m', content))).toBe('plan.pdf');
  });

  it('a deleted message should say it was deleted', () => {
    expect(messageText(withContent('$m', { type: 'Redacted' }))).toBe('This message was deleted.');
  });
});

describe('previewLine', () => {
  it('a preview should name the sender', () => {
    expect(previewLine(message('$m', 'hi'))).toBe('Grace: hi');
  });
});

describe('listTime', () => {
  it('a message from yesterday should say yesterday', () => {
    expect(listTime(NOW - 24 * 60 * 60 * 1000, NOW)).toBe('Yesterday');
  });

  it('a message from today should show the time of day', () => {
    expect(listTime(new Date(2026, 10, 16, 9, 5).getTime(), NOW)).toMatch(/9:05/);
  });
});

describe('jumpTo', () => {
  const rooms = [
    roomSummary('!design-chat', 'Chat about design'),
    roomSummary('!design', 'Design'),
    roomSummary('!other', 'Other'),
    { ...roomSummary('!dm', 'Grace Hopper', 'Direct'), dm_users: [GRACE] },
  ];

  it('a name starting with the term should come before one containing it', () => {
    expect(jumpTo(accountWith(rooms), 'design', NOW).rooms.map((r) => r.room_id)).toEqual(['!design', '!design-chat']);
  });

  it('matching should ignore case and accents', () => {
    const account = accountWith([roomSummary('!cafe', 'Café')]);
    expect(jumpTo(account, 'CAFE', NOW).rooms.map((r) => r.room_id)).toEqual(['!cafe']);
  });

  it('a direct chat should be found by the other person’s ID', () => {
    expect(jumpTo(accountWith(rooms), '@grace', NOW).direct.map((r) => r.room_id)).toEqual(['!dm']);
  });

  it('spaces should be found by name', () => {
    expect(jumpTo(accountWith(rooms), 'des', NOW).spaces.map((s) => s.room_id)).toEqual(['!s']);
  });

  it('without a term the latest conversations should be listed', () => {
    const account = { ...accountWith(rooms), latest: { '!other': { ...message('$o'), timestamp_ms: NOW - 1 } } };
    expect(jumpTo(account, ' ', NOW).rooms[0]!.room_id).toBe('!other');
  });
});

describe('activityRows', () => {
  it('a single account should not label its rows', () => {
    const state = { ...signedIn(ADA), activity: [notification(ADA, '$1')] };
    expect(activityRows(state)[0]!.accountLabel).toBeNull();
  });

  it('several accounts should label each row with its account', () => {
    const state = { ...signedIn(ADA, GRACE), activity: [notification(GRACE, '$1')] };
    expect(activityRows(state)[0]!.accountLabel).toBe('Grace');
  });

  it('notifications of an account signed out since should be left out', () => {
    const state = { ...signedIn(ADA), activity: [notification(GRACE, '$1'), notification(ADA, '$2')] };
    expect(activityRows(state).map((r) => r.notification.event_id)).toEqual(['$2']);
  });
});
