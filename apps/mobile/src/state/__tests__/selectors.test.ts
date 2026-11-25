import type { SavedMessage } from '@better-matrix/matrix-native/types';

import { apply, initialState } from '../reducer';
import { shallow } from '../react';
import {
  activeRoomList,
  identityOf,
  knownPerson,
  canStartMeeting,
  rightsKey,
  roomMeetings,
  roomWidgetCount,
  roomSettings as settingsOf,
  savedMessages,
  savedSignature,
  space,
  unnamedSenders,
} from '../selectors';
import type { TimelineDiff } from '@better-matrix/matrix-native/types';

import {
  ADA,
  GRACE,
  messageEntry,
  person,
  roomSettings,
  roomSummary,
  spaceSummary,
  summary,
} from '../../test/fixtures';

function saved(room_id: string, event_id: string): SavedMessage {
  return {
    room_id,
    saved_at_ms: 1,
    preview: {
      event_id,
      sender_name: 'Grace',
      sender_id: '@grace:x',
      sender_avatar: null,
      timestamp_ms: 1,
      body: event_id,
      thread: null,
    },
  };
}

function withSaved(ids: string[]) {
  let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
  const rooms = [{ ...roomSummary('!r:x'), saved: ids }];
  state = apply(state, {
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms, spaces: [], invitations: [] } },
  });

  return apply(state, {
    type: 'SavedMessages',
    data: { account: ADA, messages: [saved('!r:x', '$a'), saved('!r:x', '$b')] },
  });
}

describe('saved messages', () => {
  it('should drop a message no longer saved at once', () => {
    expect(savedMessages(withSaved(['$b']), ADA)?.map((m) => m.preview.event_id)).toEqual(['$b']);
  });

  it('should be unknown until listed', () => {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

    expect(savedMessages(state, ADA)).toBeUndefined();
  });

  it('should change its signature when a message is saved', () => {
    expect(savedSignature(withSaved(['$a']), ADA)).not.toBe(
      savedSignature(withSaved(['$a', '$c']), ADA),
    );
  });
});

describe('activeRoomList', () => {
  it('should stay the same while someone types', () => {
    const before = withSaved([]);

    const after = apply(before, {
      type: 'Typing',
      data: { account: ADA, room_id: '!r:x', people: [person('@grace:x')] },
    });

    expect(shallow(activeRoomList(before), activeRoomList(after))).toBe(true);
  });

  it('should change with the rooms', () => {
    const before = withSaved([]);

    const after = apply(before, {
      type: 'RoomsUpdated',
      data: { account: ADA, list: { rooms: [], spaces: [], invitations: [] } },
    });

    expect(shallow(activeRoomList(before), activeRoomList(after))).toBe(false);
  });

  const changed = (state: ReturnType<typeof withSaved>, diffs: TimelineDiff[], thread?: string) =>
    apply(state, {
      type: 'TimelineChanged',
      data: { account: ADA, key: { room_id: '!r:x', thread: thread ?? null }, diffs },
    });

  it('should stay the same while older history loads into a timeline', () => {
    const before = changed(withSaved([]), [{ type: 'Reset', data: [messageEntry('$new')] }]);
    const after = changed(before, [{ type: 'PushFront', data: messageEntry('$old') }]);
    expect(shallow(activeRoomList(before), activeRoomList(after))).toBe(true);
  });

  it('should stay the same while a thread changes', () => {
    const before = withSaved([]);
    const after = changed(before, [{ type: 'Reset', data: [messageEntry('$reply')] }], '$root');
    expect(shallow(activeRoomList(before), activeRoomList(after))).toBe(true);
  });

  it('should change when a room newest message changes', () => {
    const before = changed(withSaved([]), [{ type: 'Reset', data: [messageEntry('$old')] }]);
    const after = changed(before, [{ type: 'PushBack', data: messageEntry('$new') }]);
    expect(shallow(activeRoomList(before), activeRoomList(after))).toBe(false);
    expect(activeRoomList(after)?.newest['!r:x']?.event_id).toBe('$new');
  });
});

describe('unnamedSenders', () => {
  it('bubbles should name everyone in a room', () => {
    expect(unnamedSenders(roomSummary('!r:x'))).toEqual([]);
  });

  it('bubbles should not name the other person of a one-to-one chat', () => {
    const chat = { ...roomSummary('!d:x', 'Grace', 'Direct'), dm_users: ['@grace:x'] };
    expect(unnamedSenders(chat)).toEqual(['@grace:x']);
  });

  it('bubbles should name everyone in a direct chat a third person joined', () => {
    const chat = { ...roomSummary('!d:x', 'Grace', 'Direct'), dm_users: ['@grace:x'] };
    expect(unnamedSenders({ ...chat, member_count: 3 })).toEqual([]);
  });
});

describe('people', () => {
  function withGrace() {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

    return apply(state, {
      type: 'Members',
      data: {
        request: null,
        account: ADA,
        room_id: '!r:x',
        members: {
          people: [
            {
              person: person(GRACE, 'Grace'),
              role: 'Member',
              presence: 'Online',
              status_message: null,
              verified: false,
              level: 0,
            },
          ],
          complete: true,
          can_invite: false,
          own_level: 0,
          can_kick: false,
          can_ban: false,
          can_set_roles: false,
          knocking: [],
          banned: [],
        },
      },
    });
  }

  it('should know someone from a room’s member list', () => {
    expect(knownPerson(withGrace(), ADA, GRACE)).toEqual(person(GRACE, 'Grace'));
  });

  it('should know nobody the member lists lack', () => {
    expect(knownPerson(withGrace(), ADA, '@nobody:x')).toBeUndefined();
  });

  it('should know nothing of an identity before it is reported', () => {
    expect(identityOf(withGrace(), ADA, GRACE)).toBeUndefined();
  });

  it('should read the identity reported for someone', () => {
    const state = apply(withGrace(), {
      type: 'Identity',
      data: { account: ADA, user_id: GRACE, identity: { verified: false, changed: true } },
    });

    expect(identityOf(state, ADA, GRACE)).toEqual({ verified: false, changed: true });
  });
});

describe('rightsKey', () => {
  it('should change once the room has a successor, so open settings read it again', () => {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

    const listed = (successor: boolean) =>
      apply(state, {
        type: 'RoomsUpdated',
        data: {
          account: ADA,
          list: {
            rooms: [
              {
                ...roomSummary('!r:x'),
                successor: successor ? { room_id: '!n:x', reason: null, via: [] } : null,
              },
            ],
            spaces: [],
            invitations: [],
          },
        },
      });

    expect(rightsKey(listed(false), ADA, '!r:x')).not.toBe(rightsKey(listed(true), ADA, '!r:x'));
  });
});

describe('administration', () => {
  function withSpace() {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

    return apply(state, {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: { rooms: [], spaces: [spaceSummary('!s:x', ['!r:x'])], invitations: [] },
      },
    });
  }

  it('should find a space of an account', () => {
    expect(space(withSpace(), ADA, '!s:x')?.children).toEqual(['!r:x']);
  });

  it('should have no settings for a room until they are loaded', () => {
    expect(settingsOf(withSpace(), ADA, '!s:x')).toBeUndefined();
  });

  it('should find a room loaded settings', () => {
    const settings = roomSettings('!s:x', { space: true });
    const state = apply(withSpace(), { type: 'RoomSettings', data: { account: ADA, settings } });

    expect(settingsOf(state, ADA, '!s:x')).toBe(settings);
  });
});

describe('roomMeetings', () => {
  const widget = (id: string, meeting: 'Jitsi' | null) => ({
    id,
    name: id,
    kind: 'jitsi',
    url: `https://meet.example/${id}`,
    host: 'meet.example',
    creator: null,
    can_remove: false,
    meeting,
  });

  it('should list the meetings pinned to a room', () => {
    const state = apply(apply(initialState(), { type: 'AccountAdded', data: summary(ADA) }), {
      type: 'Widgets',
      data: {
        account: ADA,
        room_id: '!r:x',
        widgets: { widgets: [widget('board', null), widget('standup', 'Jitsi')], can_add: false },
      },
    });

    expect(roomMeetings(state, ADA, '!r:x').map((w) => w.id)).toEqual(['standup']);
  });

  it('should count every widget pinned to a room', () => {
    const state = apply(apply(initialState(), { type: 'AccountAdded', data: summary(ADA) }), {
      type: 'Widgets',
      data: {
        account: ADA,
        room_id: '!r:x',
        widgets: { widgets: [widget('board', null), widget('standup', 'Jitsi')], can_add: false },
      },
    });

    expect(roomWidgetCount(state, ADA, '!r:x')).toBe(2);
  });

  it('should count no widgets for a room whose widgets are unknown', () => {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

    expect(roomWidgetCount(state, ADA, '!r:x')).toBeUndefined();
  });

  it('should list no meetings for a room whose widgets are unknown', () => {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

    expect(roomMeetings(state, ADA, '!r:x')).toEqual([]);
  });
});

describe('canStartMeeting', () => {
  const withRoom = (can: boolean) =>
    apply(apply(initialState(), { type: 'AccountAdded', data: summary(ADA) }), {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: {
          rooms: [{ ...roomSummary('!r:x'), can_start_meeting: can }],
          spaces: [],
          invitations: [],
        },
      },
    });

  it('should let a meeting start where the room says it can', () => {
    expect(canStartMeeting(withRoom(true), ADA, '!r:x')).toBe(true);
  });

  it('should not let a meeting start where the room says it can’t', () => {
    expect(canStartMeeting(withRoom(false), ADA, '!r:x')).toBe(false);
  });

  it('should not let a meeting start in a room the account doesn’t know', () => {
    expect(canStartMeeting(withRoom(true), ADA, '!other:x')).toBe(false);
  });
});
