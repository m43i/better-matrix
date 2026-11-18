import type { SavedMessage } from '@better-matrix/matrix-native/types';

import { apply, initialState } from '../reducer';
import { shallow } from '../react';
import { activeRoomList, savedMessages, savedSignature } from '../selectors';
import type { TimelineDiff } from '@better-matrix/matrix-native/types';

import { ADA, messageEntry, person, roomSummary, summary } from '../../test/fixtures';

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
