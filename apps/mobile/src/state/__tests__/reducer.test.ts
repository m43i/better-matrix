import { apply, initialState, mediaKey } from '../reducer';
import { DEFAULT_SETTINGS } from '../settings';
import { roomKey, timelineId } from '../timeline';
import type { State } from '../types';
import { ADA, GRACE, entry, message, roomSummary, summary } from '../../test/fixtures';

function signedIn(...ids: string[]): State {
  return ids.reduce(
    (s, id) => apply(s, { type: 'AccountAdded', data: summary(id) }),
    initialState(),
  );
}

describe('apply', () => {
  it('should put the first account added on screen', () => {
    expect(signedIn(ADA).active).toBe(ADA);
  });

  it('should keep the account on screen when another is added', () => {
    expect(signedIn(ADA, GRACE).active).toBe(ADA);
  });

  it('should come back to the account last on screen once it restored', () => {
    const state = initialState({ ...DEFAULT_SETTINGS, lastAccount: GRACE });

    const after = [ADA, GRACE].reduce(
      (s, id) => apply(s, { type: 'AccountAdded', data: summary(id) }),
      state,
    );

    expect(after.active).toBe(GRACE);
  });

  it('should stop counting an account as restoring once it is added', () => {
    const restoring = apply(initialState(), { type: 'Restoring', data: [ADA, GRACE] });
    expect(apply(restoring, { type: 'AccountAdded', data: summary(ADA) }).restoring).toEqual([
      GRACE,
    ]);
  });

  it('should stop counting an account as restoring when it fails to restore', () => {
    const restoring = apply(initialState(), { type: 'Restoring', data: [ADA] });
    expect(
      apply(restoring, { type: 'AccountError', data: { account: ADA, error: 'gone' } }).restoring,
    ).toEqual([]);
  });

  it('should move to another account when the one on screen is removed', () => {
    const state = apply(signedIn(ADA, GRACE), { type: 'AccountRemoved', data: ADA });
    expect(state.active).toBe(GRACE);
  });

  it('should rename an account when its profile changes', () => {
    const state = apply(signedIn(ADA), {
      type: 'AccountChanged',
      data: summary(ADA, 'Ada Lovelace'),
    });

    expect(state.accounts[0]!.summary.display_name).toBe('Ada Lovelace');
  });

  it('should offer to sign in again when a session ends', () => {
    const state = apply(signedIn(ADA), {
      type: 'SessionEnded',
      data: { account: ADA, kept: true },
    });

    expect(state.ended).toEqual([{ account: ADA, kept: true }]);
  });

  it('should keep the room list an account synced', () => {
    const list = { rooms: [roomSummary('!a:x')], spaces: [], invitations: [] };
    const state = apply(signedIn(ADA), { type: 'RoomsUpdated', data: { account: ADA, list } });
    expect(state.accounts[0]!.rooms.map((r) => r.room_id)).toEqual(['!a:x']);
  });

  it('should go back home when the space in scope is left', () => {
    let state = signedIn(ADA);
    state = { ...state, accounts: [{ ...state.accounts[0]!, scope: { space: '!s:x' } }] };
    state = apply(state, {
      type: 'RoomsUpdated',
      data: { account: ADA, list: { rooms: [], spaces: [], invitations: [] } },
    });
    expect(state.accounts[0]!.scope).toBe('home');
  });

  it('should apply timeline diffs in order', () => {
    const key = roomKey('!a:x');

    const state = apply(signedIn(ADA), {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key,
        diffs: [
          { type: 'Reset', data: [entry('1')] },
          { type: 'PushBack', data: entry('2') },
        ],
      },
    });

    expect(state.accounts[0]!.timelines[timelineId(key)]!.entries.map((e) => e.id)).toEqual([
      '1',
      '2',
    ]);
  });

  it('should note when history reached the start', () => {
    const key = roomKey('!a:x');

    const state = apply(signedIn(ADA), {
      type: 'HistoryLoaded',
      data: { account: ADA, key, reached_start: true },
    });

    expect(state.accounts[0]!.timelines[timelineId(key)]!.reachedStart).toBe(true);
  });

  it('should keep why history failed', () => {
    const key = roomKey('!a:x');

    const state = apply(signedIn(ADA), {
      type: 'HistoryFailed',
      data: { account: ADA, key, error: 'offline' },
    });

    expect(state.accounts[0]!.timelines[timelineId(key)]!.historyError).toBe('offline');
  });

  it('should note when it heard of a room newest message', () => {
    const state = apply(
      signedIn(ADA),
      { type: 'LatestMessage', data: { account: ADA, room_id: '!a:x', message: message('$1') } },
      42,
    );

    expect(state.accounts[0]!.heardAt['!a:x']).toBe(42);
  });

  it('should keep the newer of two sidebar layouts', () => {
    let state = apply(signedIn(ADA), {
      type: 'SidebarLayout',
      data: { account: ADA, layout: { scopes: {}, changed_at: 10 } },
    });

    state = apply(state, {
      type: 'SidebarLayout',
      data: { account: ADA, layout: { scopes: {}, changed_at: 5 } },
    });
    expect(state.accounts[0]!.sidebar.changed_at).toBe(10);
  });

  it('should keep a fetched media file by what it is', () => {
    const media = 'mxc://example.org/a';
    const size = { type: 'Original' as const };

    const state = apply(initialState(), {
      type: 'MediaFile',
      data: { request: 1, media, size, path: '/c/a.png' },
    });

    expect(state.media[mediaKey(media, size)]).toBe('/c/a.png');
  });

  it('should keep the latest step of each verification', () => {
    let state = apply(signedIn(ADA), {
      type: 'Verification',
      data: { account: ADA, flow_id: 'f', step: { type: 'Waiting' } },
    });

    state = apply(state, {
      type: 'Verification',
      data: { account: ADA, flow_id: 'f', step: { type: 'Done' } },
    });
    expect(state.verifications).toEqual([{ account: ADA, flowId: 'f', step: { type: 'Done' } }]);
  });

  it('should count notifications not seen yet', () => {
    const notification = {
      account: ADA,
      room_id: '!a:x',
      room_name: 'a',
      event_id: '$1',
      sender_name: 'Grace',
      body: 'hi',
    };

    const state = apply(apply(initialState(), { type: 'Notification', data: notification }), {
      type: 'Notification',
      data: notification,
    });

    expect(state.activityUnseen).toBe(2);
  });

  it('should keep a room read boundary that is outside loaded history as unresolved', () => {
    const state = apply(signedIn(ADA), {
      type: 'ReadBoundary',
      data: { account: ADA, room_id: '!a:x', event_id: '$9', loaded: false },
    });

    expect(state.accounts[0]!.timelines['!a:x']!.readBoundary).toEqual({
      kind: 'unresolved',
      eventId: '$9',
    });
  });

  it('should note a room without a read boundary as absent', () => {
    const state = apply(signedIn(ADA), {
      type: 'ReadBoundary',
      data: { account: ADA, room_id: '!a:x', event_id: null, loaded: false },
    });

    expect(state.accounts[0]!.timelines['!a:x']!.readBoundary).toEqual({ kind: 'absent' });
  });

  it('should forget a room members once sync says they changed', () => {
    const members = {
      people: [],
      complete: true,
      can_invite: false,
      own_level: 0,
      can_kick: false,
      can_ban: false,
      can_set_roles: false,
    };

    let state = apply(signedIn(ADA), {
      type: 'Members',
      data: { request: null, account: ADA, room_id: '!a:x', members },
    });

    state = apply(state, { type: 'MembersInvalidated', data: { account: ADA, room_id: '!a:x' } });
    expect(state.accounts[0]!.members['!a:x']).toBeUndefined();
  });

  it('should keep the invitations the room list carries', () => {
    const invitation = {
      room_id: '!i:x',
      name: 'Invited',
      topic: null,
      avatar: null,
      inviter: null,
      space: false,
      kind: 'Group' as const,
    };

    const list = { rooms: [], spaces: [], invitations: [invitation] };
    const state = apply(signedIn(ADA), { type: 'RoomsUpdated', data: { account: ADA, list } });
    expect(state.accounts[0]!.invitations).toEqual([invitation]);
  });

  it('should show an account error as a notice', () => {
    const state = apply(signedIn(ADA), {
      type: 'AccountError',
      data: { account: ADA, error: 'sync failed' },
    });

    expect(state.notices.map((n) => n.message)).toEqual(['sync failed']);
  });

  it('should ring once per room however often the ring is reported', () => {
    const ring = {
      account: ADA,
      room_id: '!a:x',
      caller: { id: GRACE, name: 'Grace', avatar: null },
      video: false,
      via: 'Direct' as const,
    };

    let state = apply(initialState(), { type: 'IncomingCall', data: ring });
    state = apply(state, { type: 'IncomingCall', data: ring });
    expect(state.call.rings).toHaveLength(1);
  });

  it('should stop ringing when the ring ends', () => {
    const ring = {
      account: ADA,
      room_id: '!a:x',
      caller: { id: GRACE, name: 'Grace', avatar: null },
      video: false,
      via: 'Direct' as const,
    };

    let state = apply(initialState(), { type: 'IncomingCall', data: ring });
    state = apply(state, { type: 'RingEnded', data: { account: ADA, room_id: '!a:x' } });
    expect(state.call.rings).toEqual([]);
  });

  it('should clear the call when it ends and keep why', () => {
    const info = {
      call: 3,
      account: ADA,
      room_id: '!a:x',
      via: 'Rtc' as const,
      phase: 'Connected' as const,
      video: false,
      microphone: true,
      camera: null,
      screen: null,
      participants: [],
    };

    let state = apply(initialState(), { type: 'CallUpdated', data: info });
    state = apply(state, {
      type: 'CallEnded',
      data: { account: ADA, room_id: '!a:x', call: 3, reason: { type: 'HungUp' } },
    });
    expect([state.call.current, state.call.ended?.reason]).toEqual([null, { type: 'HungUp' }]);
  });
});

describe('pushers', () => {
  it('a removed account should take its pusher registration with it', () => {
    let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
    state = { ...state, pushers: { [ADA]: { token: 't', gateway: 'g', lang: 'en' } } };
    expect(apply(state, { type: 'AccountRemoved', data: ADA }).pushers).toEqual({});
  });

  it('a pushed event fetched for a request should change nothing', () => {
    const state = initialState();
    expect(
      apply(state, { type: 'PushContent', data: { request: 1, content: { type: 'Nothing' } } }),
    ).toBe(state);
  });
});
