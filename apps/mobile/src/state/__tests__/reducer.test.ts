import {
  applyLatestMessages,
  apply,
  editsKey,
  forgetRoomDetails,
  initialState,
  mediaKey,
} from '../reducer';
import { DEFAULT_SETTINGS } from '../settings';
import { roomKey, timelineId } from '../timeline';
import type { State } from '../types';
import {
  ADA,
  GRACE,
  entry,
  message,
  roomSettings,
  roomSummary,
  spaceSummary,
  summary,
} from '../../test/fixtures';

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

  it('should know no keywords for an account until the engine reports them', () => {
    expect(signedIn(ADA).accounts[0]!.keywords).toBeNull();
  });

  it('should show the keywords the engine reports for an account', () => {
    const state = apply(signedIn(ADA), {
      type: 'Keywords',
      data: { account: ADA, keywords: ['deploy'] },
    });

    expect(state.accounts[0]!.keywords).toEqual(['deploy']);
  });

  it('should show the status the engine reports for an account', () => {
    const state = apply(signedIn(ADA), { type: 'Status', data: { account: ADA, status: 'Away' } });

    expect(state.accounts[0]!.status).toBe('Away');
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
    expect(state.verifications).toEqual([
      { account: ADA, flowId: 'f', step: { type: 'Done' }, user: null },
    ]);
  });

  it('should remember who asked to verify through the later steps', () => {
    const grace = { id: GRACE, name: 'Grace', avatar: null };

    let state = apply(signedIn(ADA), {
      type: 'Verification',
      data: {
        account: ADA,
        flow_id: 'f',
        step: { type: 'Incoming', data: { other_device: 'GRACEPHONE', user: grace } },
      },
    });

    state = apply(state, {
      type: 'Verification',
      data: { account: ADA, flow_id: 'f', step: { type: 'Ready', data: { qr: null, scan: true } } },
    });
    expect(state.verifications[0]?.user).toEqual(grace);
  });

  it('should keep the step after the other side scanned the code', () => {
    const state = apply(signedIn(ADA), {
      type: 'Verification',
      data: { account: ADA, flow_id: 'f', step: { type: 'Scanned' } },
    });

    expect(state.verifications[0]?.step).toEqual({ type: 'Scanned' });
  });

  it('should keep the identity reported for a person', () => {
    const state = apply(signedIn(ADA), {
      type: 'Identity',
      data: { account: ADA, user_id: GRACE, identity: { verified: true, changed: false } },
    });

    expect(state.accounts[0]?.identities[GRACE]).toEqual({ verified: true, changed: false });
  });

  it('should remember that a person has no identity to verify', () => {
    const state = apply(signedIn(ADA), {
      type: 'Identity',
      data: { account: ADA, user_id: GRACE, identity: null },
    });

    expect(state.accounts[0]?.identities).toEqual({ [GRACE]: null });
  });

  it('should keep whether the offline device is on', () => {
    const state = apply(signedIn(ADA), {
      type: 'SecurityChanged',
      data: {
        account: ADA,
        security: { session_verified: true, recovery: 'Enabled', offline_device: 'On' },
      },
    });

    expect(state.accounts[0]?.security?.offline_device).toBe('On');
  });

  it('should leave the state alone for a QR sign-in step, which only its request hears', () => {
    const state = signedIn(ADA);
    expect(apply(state, { type: 'QrLogin', data: { request: 3, step: { type: 'Syncing' } } })).toBe(
      state,
    );
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

  it('should keep a room settings by its room ID', () => {
    const settings = roomSettings('!a:x');
    const state = apply(signedIn(ADA), { type: 'RoomSettings', data: { account: ADA, settings } });

    expect(state.accounts[0]!.roomSettings['!a:x']).toBe(settings);
  });

  it('should replace a room settings with fresher ones', () => {
    let state = apply(signedIn(ADA), {
      type: 'RoomSettings',
      data: { account: ADA, settings: roomSettings('!a:x') },
    });

    state = apply(state, {
      type: 'RoomSettings',
      data: { account: ADA, settings: roomSettings('!a:x', { join_rule: 'Public' }) },
    });

    expect(state.accounts[0]!.roomSettings['!a:x']?.join_rule).toBe('Public');
  });

  it('should keep those asking to join and those banned with a room members', () => {
    const asking = {
      person: { id: GRACE, name: 'Grace', avatar: null },
      role: 'Member' as const,
      presence: 'Unknown' as const,
      status_message: null,
      verified: false,
      level: 0,
    };

    const state = apply(signedIn(ADA), {
      type: 'Members',
      data: {
        request: null,
        account: ADA,
        room_id: '!a:x',
        members: {
          people: [],
          complete: true,
          can_invite: true,
          own_level: 100,
          can_kick: true,
          can_ban: true,
          can_set_roles: true,
          knocking: [asking],
          banned: [],
        },
      },
    });

    expect(state.accounts[0]!.members['!a:x']?.knocking).toEqual([asking]);
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
      knocking: [],
      banned: [],
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
      stranger: false,
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

describe('room lists', () => {
  const listed = (state: State, rooms = [roomSummary('!a:x'), roomSummary('!b:x')]) =>
    apply(state, {
      type: 'RoomsUpdated',
      data: { account: ADA, list: { rooms, spaces: [spaceSummary('!s:x')], invitations: [] } },
    });

  it('a room that did not change should keep its object when another one changes', () => {
    const before = listed(signedIn(ADA));

    const after = listed(before, [roomSummary('!a:x'), { ...roomSummary('!b:x'), unread: 3 }]);

    expect(after.accounts[0]!.rooms[0]).toBe(before.accounts[0]!.rooms[0]);
    expect(after.accounts[0]!.rooms[1]).not.toBe(before.accounts[0]!.rooms[1]);
    expect(after.accounts[0]!.rooms[1]!.unread).toBe(3);
  });

  it('a list that did not change should leave the state as it was', () => {
    const before = listed(signedIn(ADA));
    expect(listed(before)).toBe(before);
  });

  it('a list in a new order should keep its rooms in that order', () => {
    const before = listed(signedIn(ADA));
    const after = listed(before, [roomSummary('!b:x'), roomSummary('!a:x')]);
    expect(after.accounts[0]!.rooms.map((r) => r.room_id)).toEqual(['!b:x', '!a:x']);
    expect(after.accounts[0]!.rooms[0]).toBe(before.accounts[0]!.rooms[1]);
  });
});

describe('latest messages', () => {
  const latest = (room_id: string, id: string) =>
    ({
      type: 'LatestMessage',
      data: { account: ADA, room_id, message: message(id) },
    }) as const;

  it('a run of them should fold in as one by one', () => {
    const state = apply(signedIn(ADA), latest('!a:x', '$0'), 1);
    const events = [latest('!a:x', '$0'), latest('!b:x', '$1'), latest('!a:x', '$2')];
    const oneByOne = events.reduce((s, event) => apply(s, event, 7), state);
    expect(applyLatestMessages(state, events, 7)).toEqual(oneByOne);
  });

  it('a message heard again should keep when it was first heard', () => {
    const state = apply(signedIn(ADA), latest('!a:x', '$0'), 1);
    expect(applyLatestMessages(state, [latest('!a:x', '$0')], 9).accounts[0]!.heardAt).toEqual({
      '!a:x': 1,
    });
  });

  it('messages for an account not signed in should change nothing', () => {
    const state = signedIn(GRACE);
    expect(applyLatestMessages(state, [latest('!a:x', '$0')])).toBe(state);
  });
});

describe('call levels', () => {
  const levels = (data: Array<[string, number]>) => ({ type: 'CallLevels', data }) as const;
  const me: [string, number] = ['', 12];
  const bob: [string, number] = ['@b', 3];

  it('levels as they were should leave the state as it was', () => {
    const state = apply(signedIn(ADA), levels([me, bob]));
    expect(apply(state, levels([bob, me]))).toBe(state);
  });

  it('a changed level should be kept', () => {
    const state = apply(signedIn(ADA), levels([me]));
    expect(apply(state, levels([['', 40]])).call.levels).toEqual({ '': 40 });
  });

  it('someone gone from the levels should be dropped', () => {
    const state = apply(signedIn(ADA), levels([me, bob]));
    expect(apply(state, levels([me])).call.levels).toEqual({ '': 12 });
  });
});

describe('forgetRoomDetails', () => {
  it('should keep an account that holds nothing about the room as it was', () => {
    const account = signedIn(ADA).accounts[0]!;
    expect(forgetRoomDetails(account, '!a:x')).toBe(account);
  });

  it('should leave the details of other rooms alone', () => {
    const state = apply(signedIn(ADA), {
      type: 'Threads',
      data: { account: ADA, room_id: '!b:x', threads: [] },
    });

    expect(forgetRoomDetails(state.accounts[0]!, '!a:x').threads).toEqual({ '!b:x': [] });
  });
});

describe('apply for composer extras', () => {
  it('should keep image packs by the room they were listed for', () => {
    const state = apply(signedIn(ADA), {
      type: 'ImagePacks',
      data: { account: ADA, room_id: '!r', packs: [] },
    });

    expect(state.accounts[0]?.packs).toEqual({ '!r': [] });
  });

  it('should keep the scheduled messages the server reported', () => {
    const messages = [{ delay_id: 'd', room_id: '!r', send_at_ms: 1, body: 'hi', failed: null }];

    const state = apply(signedIn(ADA), {
      type: 'ScheduledMessages',
      data: { account: ADA, messages },
    });

    expect(state.accounts[0]?.scheduled).toEqual(messages);
  });

  it('should tell once that a scheduled message couldn’t be sent', () => {
    const messages = [
      { delay_id: 'd', room_id: '!r', send_at_ms: 1, body: 'hi', failed: 'No posting' },
    ];

    const event = { type: 'ScheduledMessages' as const, data: { account: ADA, messages } };

    const once = apply(signedIn(ADA), event);
    const twice = apply(once, event);

    expect(once.notices.map((notice) => notice.message)).toEqual([
      expect.stringContaining('No posting'),
    ]);
    expect(twice.notices).toHaveLength(1);
  });

  it('should keep a message’s edit history by room and event', () => {
    const versions = [{ event_id: '$e', body: 'a', html: null, timestamp_ms: 1 }];

    const state = apply(signedIn(ADA), {
      type: 'EditHistory',
      data: { account: ADA, room_id: '!r', event_id: '$e', versions },
    });

    expect(state.accounts[0]?.edits[editsKey('!r', '$e')]).toEqual(versions);
  });

  it('should leave answers to one request to whoever asked', () => {
    const state = signedIn(ADA);

    expect(apply(state, { type: 'PlacesFound', data: { request: 1, places: [] } })).toBe(state);
  });

  it('should leave a widget run’s page to the screen that runs it', () => {
    const state = signedIn(ADA);
    const page = { url: 'https://w.example/', title: 'Board', bridge: 'WidgetApi' as const };

    expect(apply(state, { type: 'WidgetPage', data: { request: 1, page } })).toBe(state);
  });

  it('should keep the widgets the engine reports for a room', () => {
    const widgets = { widgets: [], can_add: true };

    const state = apply(signedIn(ADA), {
      type: 'Widgets',
      data: { account: ADA, room_id: '!r:x', widgets },
    });

    expect(state.accounts[0]?.widgets['!r:x']).toBe(widgets);
  });
});
