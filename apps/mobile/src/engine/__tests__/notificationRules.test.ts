import type { Notification } from '@better-matrix/matrix-native/types';

import { ADA, roomSummary, summary } from '../../test/fixtures';
import { apply, initialState } from '../../state/reducer';
import { contentOf, pushContentOf, shouldPresent, tapRoute, targetOf } from '../notificationRules';

function notification(change: Partial<Notification> = {}): Notification {
  return {
    account: ADA,
    room_id: '!r',
    room_name: 'Team',
    event_id: '$e',
    sender_name: 'Grace',
    body: 'hi',
    ...change,
  };
}

describe('shouldPresent', () => {
  it('a notification for the room on screen should stay quiet while the app is in front', () => {
    expect(shouldPresent(true, true)).toBe(false);
  });

  it('a notification for another room should show while the app is in front', () => {
    expect(shouldPresent(true, false)).toBe(true);
  });

  it('a notification should show while the app is in the background', () => {
    expect(shouldPresent(false, true)).toBe(true);
  });
});

describe('contentOf', () => {
  it('a notification should be titled by its room and name its sender', () => {
    const content = contentOf(notification());
    expect([content.title, content.body]).toEqual(['Team', 'Grace: hi']);
  });

  it('a direct chat named after the sender should not repeat the name', () => {
    expect(contentOf(notification({ room_name: 'Grace' })).body).toBe('hi');
  });

  it('the same event should notify once', () => {
    expect(contentOf(notification()).identifier).toBe(contentOf(notification()).identifier);
  });
});

describe('targetOf', () => {
  it('a tapped notification should lead to its room and event', () => {
    expect(targetOf(contentOf(notification()).data)).toEqual({
      account: ADA,
      roomId: '!r',
      eventId: '$e',
    });
  });

  it('a notification may mix the app and gateway field names', () => {
    expect(targetOf({ account: ADA, roomId: '!r', event_id: '$e' })).toEqual({
      account: ADA,
      roomId: '!r',
      eventId: '$e',
    });
  });

  it.each([null, [], 7, { account: ADA, room_id: 7, event_id: '$e' }])(
    'malformed notification data should lead nowhere: %j',
    (data) => {
      expect(targetOf(data)).toBeNull();
    },
  );

  it('a notification of another kind should lead nowhere', () => {
    expect(targetOf({ url: 'https://example.org' })).toBeNull();
  });
});

describe('pushContentOf', () => {
  it('a pushed message should read as the sync would show it', () => {
    expect(pushContentOf({ type: 'Message', data: notification() })).toEqual(
      contentOf(notification()),
    );
  });

  it('an unreadable push should say something generic in its room', () => {
    const shown = pushContentOf({
      type: 'Hidden',
      data: { account: ADA, room_id: '!r', room_name: 'Team', event_id: '$e' },
    });

    expect([shown?.title, shown?.body]).toEqual(['Team', 'New message']);
  });

  it('an unreadable push from an unknown room should be titled by the app', () => {
    const shown = pushContentOf({
      type: 'Hidden',
      data: { account: ADA, room_id: '!r', room_name: null, event_id: '$e' },
    });

    expect(shown?.title).toBe('Better Matrix');
  });

  it('an invite should name who invited', () => {
    const invite = {
      account: ADA,
      room_id: '!r',
      room_name: 'Team',
      event_id: '$e',
      sender_name: 'Grace',
    };

    expect(pushContentOf({ type: 'Invite', data: invite })?.body).toBe('Grace invited you');
  });

  it('a push that turned out to be nothing should show nothing', () => {
    expect(pushContentOf({ type: 'Nothing' })).toBeNull();
  });
});

describe('targetOf a push', () => {
  it('a tapped push should lead to its room and event', () => {
    expect(targetOf({ account: ADA, room_id: '!r', event_id: '$e', unread: 1 })).toEqual({
      account: ADA,
      roomId: '!r',
      eventId: '$e',
    });
  });
});

describe('tapRoute', () => {
  const target = { account: ADA, roomId: '!new:x', eventId: '$invite' };
  const signedIn = () => apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

  const listed = (invited: boolean) =>
    apply(signedIn(), {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: {
          rooms: invited ? [] : [roomSummary('!new:x')],
          spaces: [],
          invitations: invited
            ? [
                {
                  room_id: '!new:x',
                  name: 'New',
                  topic: null,
                  avatar: null,
                  space: false,
                  kind: 'Group',
                  inviter: null,
                },
              ]
            : [],
        },
      },
    });

  it('a tapped invitation should open its sheet', () => {
    expect(tapRoute(listed(true), target)).toBe('invitation');
  });

  it('a tapped message should open its room', () => {
    expect(tapRoute(listed(false), target)).toBe('room');
  });

  it('a tap before the room list is in should wait', () => {
    expect(tapRoute(signedIn(), target)).toBe('wait');
  });

  it('a tap the cached room list knows nothing about should wait for sync', () => {
    const cached = apply(signedIn(), {
      type: 'RoomsUpdated',
      data: { account: ADA, list: { rooms: [roomSummary('!old:x')], spaces: [], invitations: [] } },
    });

    expect(tapRoute(cached, target)).toBe('wait');
  });

  it('a tap that waited out sync should open the room', () => {
    expect(tapRoute(signedIn(), target, true)).toBe('room');
  });
});
