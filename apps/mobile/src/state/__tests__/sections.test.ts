import type { RoomSummary, SidebarLayout, SpaceSummary } from '@better-matrix/matrix-native/types';

import {
  ADA,
  message,
  messageEntry,
  roomSummary,
  spaceSummary,
  summary,
  timelineOf,
} from '../../test/fixtures';
import { apply, initialState } from '../reducer';
import {
  activity,
  addSection,
  byRecency,
  DIRECT,
  deleteSection,
  directChats,
  FAVOURITES,
  hasUnread,
  lastMessage,
  LOW_PRIORITY,
  moveRoom,
  moveSection,
  moveSpace,
  newestMessages,
  noteToSelf,
  orderedSpaces,
  priorityIn,
  recentConversations,
  ROOMS,
  roomsInScope,
  sectionOf,
  sections,
  stamped,
  toggleCollapsed,
  unreadRooms,
  wantsAttention,
} from '../sections';
import type { AccountData } from '../types';

const NOW = 1_800_000_000_000;

const EMPTY: SidebarLayout = { scopes: {}, changed_at: 0 };

it('the Overview should show recent rooms before their latest messages arrive', () => {
  const account = accountWith([
    room('!old', { activity_ms: NOW - 1_000 }),
    room('!quiet'),
    room('!new', { activity_ms: NOW - 500 }),
  ]);

  expect(ids(recentConversations(account, 'home', NOW))).toEqual(['!new', '!old']);
});

function accountWith(
  rooms: RoomSummary[],
  spaces: SpaceSummary[] = [],
  change: Partial<AccountData> = {},
): AccountData {
  let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
  state = apply(state, {
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms, spaces, invitations: [] } },
  });

  return { ...state.accounts[0]!, ...change };
}

function room(id: string, change: Partial<RoomSummary> = {}): RoomSummary {
  return { ...roomSummary(id), ...change };
}

function dm(id: string, with_: string, change: Partial<RoomSummary> = {}): RoomSummary {
  return { ...roomSummary(id, id, 'Direct'), dm_users: [with_], ...change };
}

const ids = (rooms: readonly RoomSummary[]) => rooms.map((r) => r.room_id);

describe('wantsAttention', () => {
  it('a room notifying for all messages should want attention while it has unread ones', () => {
    expect(wantsAttention(room('!a', { unread: 2 }))).toBe(true);
  });

  it('a room notifying for mentions only should not want attention for plain unread messages', () => {
    expect(wantsAttention(room('!a', { unread: 2, notifications: 'MentionsOnly' }))).toBe(false);
  });

  it('a muted room should want attention when it mentions the user', () => {
    expect(wantsAttention(room('!a', { unread: 2, mentions: 1, notifications: 'Mute' }))).toBe(
      true,
    );
  });

  it('a room the user marked unread should want attention even when muted', () => {
    expect(wantsAttention(room('!a', { marked_unread: true, notifications: 'Mute' }))).toBe(true);
  });
});

describe('roomsInScope', () => {
  const spaces = [
    spaceSummary('!space', ['!inside', '!sub'], ['@member:x']),
    spaceSummary('!sub', ['!deep']),
  ];

  const account = accountWith(
    [
      room('!inside'),
      room('!deep'),
      room('!outside'),
      dm('!dm-member', '@member:x'),
      dm('!dm-other', '@other:x'),
    ],
    spaces,
  );

  it('home should list the group rooms in no space', () => {
    expect(ids(roomsInScope(account, 'home', 'Group'))).toEqual(['!outside']);
  });

  it('home should list every direct chat', () => {
    expect(ids(roomsInScope(account, 'home', 'Direct'))).toEqual(['!dm-member', '!dm-other']);
  });

  it('a space should list its rooms and those of its subspaces', () => {
    expect(ids(roomsInScope(account, { space: '!space' }, 'Group'))).toEqual(['!inside', '!deep']);
  });

  it('a space should list direct chats with its members', () => {
    expect(ids(roomsInScope(account, { space: '!space' }, 'Direct'))).toEqual(['!dm-member']);
  });

  it('spaces containing each other should not loop', () => {
    const looped = accountWith(
      [room('!r')],
      [spaceSummary('!a', ['!b', '!r']), spaceSummary('!b', ['!a'])],
    );

    expect(ids(roomsInScope(looped, { space: '!b' }))).toEqual(['!r']);
  });

  it('the same rooms and spaces should give the same list', () => {
    expect(roomsInScope({ ...account }, 'home')).toBe(roomsInScope(account, 'home'));
  });

  it('a new rooms list should be listed afresh', () => {
    const changed = { ...account, rooms: [...account.rooms, room('!new')] };

    expect(ids(roomsInScope(changed, 'home', 'Group'))).toEqual(['!outside', '!new']);
  });

  it('a new spaces list should be listed afresh', () => {
    const changed = { ...account, spaces: [spaceSummary('!space', ['!outside'])] };

    expect(ids(roomsInScope(changed, { space: '!space' }, 'Group'))).toEqual(['!outside']);
  });
});

describe('noteToSelf', () => {
  const notes = room('!notes', { kind: 'Direct', note_to_self: true, priority: 'Favourite' });

  // Even one the user put in a space.
  const account = accountWith(
    [notes, room('!r'), dm('!dm', '@b:x')],
    [spaceSummary('!space', ['!r', '!notes'], ['@b:x'])],
  );

  it('the note to self should stand above home', () => {
    expect(noteToSelf(account, 'home')?.room_id).toBe('!notes');
  });

  it('a space should not list the note to self', () => {
    expect(noteToSelf(account, { space: '!space' })).toBeUndefined();
    expect(ids(roomsInScope(account, { space: '!space' }))).not.toContain('!notes');
  });

  it('the note to self should not also be listed among the unread rooms or the direct chats', () => {
    const unread = accountWith([{ ...notes, marked_unread: true }, dm('!dm', '@b:x')]);

    expect(ids(unreadRooms(unread, 'home'))).not.toContain('!notes');
    expect(ids(directChats(unread, NOW))).toEqual(['!dm']);
  });

  it('an account without a note to self should show none', () => {
    expect(noteToSelf(accountWith([room('!r')]), 'home')).toBeUndefined();
  });

  it('the note to self should be in no section, even as a favourite', () => {
    const listed = sections(account, 'home', NOW).flatMap((view) => ids(view.rooms));

    expect(listed).not.toContain('!notes');
    expect(listed).toEqual(['!dm']);
  });

  it('a room no longer the note to self should be listed like any other', () => {
    const shared = accountWith([room('!notes', { kind: 'Group' })]);

    expect(noteToSelf(shared, 'home')).toBeUndefined();
    expect(sections(shared, 'home', NOW).find((view) => view.id === ROOMS)?.rooms).toHaveLength(1);
  });
});

describe('hasUnread', () => {
  it('a scope should have unread activity when one of its rooms wants attention', () => {
    const account = accountWith([room('!a', { unread: 1 })]);
    expect(hasUnread(account, 'home')).toBe(true);
  });

  it('rooms in a space should not count for home', () => {
    const account = accountWith([room('!a', { unread: 1 })], [spaceSummary('!s', ['!a'])]);
    expect(hasUnread(account, 'home')).toBe(false);
  });

  it('a scope should have unread activity once its rooms list changes to some', () => {
    const account = accountWith([room('!a')]);
    hasUnread(account, 'home');

    expect(hasUnread({ ...account, rooms: [room('!a', { unread: 1 })] }, 'home')).toBe(true);
  });
});

describe('lastMessage', () => {
  it('the newer of the open timeline and the synced message should win', () => {
    const account = accountWith([room('!a')], [], {
      latest: { '!a': { ...message('$old'), timestamp_ms: 1 } },
      timelines: { '!a': timelineOf([messageEntry('$new', { timestamp_ms: 2 })]) },
    });

    expect(lastMessage(account, '!a')?.event_id).toBe('$new');
  });
});

describe('newestMessages', () => {
  const withTimelines = (timelines: AccountData['timelines']) =>
    accountWith([room('!a')], [], { timelines });

  it('should hold each room newest message in its timeline', () => {
    const account = withTimelines({
      '!a': timelineOf([messageEntry('$old'), messageEntry('$new')]),
    });

    expect(newestMessages(account)['!a']?.event_id).toBe('$new');
  });

  it('should leave threads out', () => {
    const account = withTimelines({ '!a|$root': timelineOf([messageEntry('$reply')]) });
    expect(newestMessages(account)).toEqual({});
  });

  it('should stay the same while older history loads', () => {
    const before = newestMessages(withTimelines({ '!a': timelineOf([messageEntry('$new')]) }));

    const after = newestMessages(
      withTimelines({ '!a': timelineOf([messageEntry('$old'), messageEntry('$new')]) }),
    );

    expect(after).toBe(before);
  });

  it('should stay the same when someone reacts to the newest message', () => {
    const before = newestMessages(withTimelines({ '!a': timelineOf([messageEntry('$new')]) }));

    const reaction = { key: '👍', count: 1, mine: false };

    const after = newestMessages(
      withTimelines({ '!a': timelineOf([messageEntry('$new', { reactions: [reaction] })]) }),
    );

    expect(after).toBe(before);
  });

  it('should change when a newer message arrives', () => {
    const before = newestMessages(withTimelines({ '!a': timelineOf([messageEntry('$old')]) }));

    const after = newestMessages(
      withTimelines({ '!a': timelineOf([messageEntry('$old'), messageEntry('$new')]) }),
    );

    expect(after).not.toBe(before);
    expect(after['!a']?.event_id).toBe('$new');
  });

  it('should change when the newest message is edited', () => {
    const before = newestMessages(withTimelines({ '!a': timelineOf([messageEntry('$m')]) }));

    const edited = newestMessages(
      withTimelines({
        '!a': timelineOf([
          messageEntry('$m', { content: { type: 'Text', data: { body: 'fixed', html: null } } }),
        ]),
      }),
    );

    expect(lastMessage({ newest: edited, latest: {} }, '!a')?.content).toEqual({
      type: 'Text',
      data: { body: 'fixed', html: null },
    });
    expect(edited).not.toBe(before);
  });

  it('should order rooms as their timelines do', () => {
    const account = accountWith([room('!a'), room('!b')], [], {
      timelines: {
        '!a': timelineOf([messageEntry('$a', { timestamp_ms: NOW - 20 })]),
        '!b': timelineOf([messageEntry('$b', { timestamp_ms: NOW - 10 })]),
      },
    });

    const fromNewest = { ...account, newest: newestMessages(account) };
    expect(ids(byRecency(fromNewest, account.rooms, NOW))).toEqual(['!b', '!a']);
    expect(ids(byRecency(account, account.rooms, NOW))).toEqual(['!b', '!a']);
  });
});

describe('activity', () => {
  it('a room should count from its newest message', () => {
    const account = accountWith([room('!a')], [], {
      latest: { '!a': { ...message('$m'), timestamp_ms: NOW - 10 } },
    });

    expect(activity(account, account.rooms[0]!, NOW)).toBe(NOW - 10);
  });

  it('a message dated far in the future should count from when it was heard', () => {
    const account = accountWith([room('!a')], [], {
      latest: { '!a': { ...message('$m'), timestamp_ms: NOW + 3_600_000 } },
      heardAt: { '!a': NOW - 5 },
    });

    expect(activity(account, account.rooms[0]!, NOW)).toBe(NOW - 5);
  });

  it('a room without messages should count from the activity the store kept', () => {
    const account = accountWith([room('!a', { activity_ms: NOW - 99 })]);
    expect(activity(account, account.rooms[0]!, NOW)).toBe(NOW - 99);
  });
});

describe('sections', () => {
  const at = (id: string, ms: number) => ({ [id]: { ...message(`$${id}`), timestamp_ms: ms } });

  it('the built-in sections should list the latest conversation first', () => {
    const account = accountWith(
      [room('!old'), room('!new'), room('!quiet', { name: 'quiet' })],
      [],
      {
        latest: { ...at('!old', NOW - 100), ...at('!new', NOW - 1) },
      },
    );

    const rooms = sections(account, 'home', NOW).find((s) => s.id === ROOMS)!.rooms;
    expect(ids(rooms)).toEqual(['!new', '!old', '!quiet']);
  });

  it('rooms with nothing known should follow by name', () => {
    const account = accountWith([room('!b', { name: 'Beta' }), room('!a', { name: 'alpha' })]);
    expect(ids(sections(account, 'home', NOW).find((s) => s.id === ROOMS)!.rooms)).toEqual([
      '!a',
      '!b',
    ]);
  });

  it('a room in a section of the user should leave the built-in one', () => {
    const { layout, id } = addSection(EMPTY, 'home', 'Team');

    const account = accountWith([room('!a'), room('!b')], [], {
      sidebar: moveRoom(layout, 'home', '!a', id),
    });

    const shown = sections(account, 'home', NOW);
    expect(shown.map((s) => [s.id, ids(s.rooms)])).toEqual([
      [FAVOURITES, []],
      [id, ['!a']],
      [ROOMS, ['!b']],
      [DIRECT, []],
      [LOW_PRIORITY, []],
    ]);
  });

  it('the user section should keep its rooms in the order they were placed', () => {
    let { layout, id } = addSection(EMPTY, 'home', 'Team');
    layout = moveRoom(moveRoom(layout, 'home', '!b', id), 'home', '!a', id);
    const account = accountWith([room('!a'), room('!b')], [], { sidebar: layout });
    expect(ids(sections(account, 'home', NOW).find((s) => s.id === id)!.rooms)).toEqual([
      '!b',
      '!a',
    ]);
  });

  it('sections should follow the saved order', () => {
    const { layout } = addSection(EMPTY, 'home', 'Team');
    const ordered = moveSection(layout, 'home', ['section-1', ROOMS, DIRECT], DIRECT, 0);
    const account = accountWith([], [], { sidebar: ordered });
    expect(sections(account, 'home', NOW).map((s) => s.id)).toEqual([
      FAVOURITES,
      DIRECT,
      'section-1',
      ROOMS,
      LOW_PRIORITY,
    ]);
  });

  it('a collapsed section should say so', () => {
    const account = accountWith([], [], { sidebar: toggleCollapsed(EMPTY, 'home', DIRECT) });
    expect(sections(account, 'home', NOW).find((s) => s.id === DIRECT)!.collapsed).toBe(true);
  });

  it('a section should hide rooms outside the scope', () => {
    const { layout, id } = addSection(EMPTY, 'home', 'Team');

    const account = accountWith([room('!in-space')], [spaceSummary('!s', ['!in-space'])], {
      sidebar: moveRoom(layout, 'home', '!in-space', id),
    });

    expect(sections(account, 'home', NOW).find((s) => s.id === id)!.rooms).toEqual([]);
  });

  it('favourites should come first and low priority rooms last', () => {
    const account = accountWith([
      room('!low', { priority: 'Low' }),
      room('!plain'),
      dm('!fav', '@g:x', { priority: 'Favourite' }),
    ]);

    expect(sections(account, 'home', NOW).map((s) => [s.id, ids(s.rooms)])).toEqual([
      [FAVOURITES, ['!fav']],
      [ROOMS, ['!plain']],
      [DIRECT, []],
      [LOW_PRIORITY, ['!low']],
    ]);
  });

  it('a favourite should leave the user section holding it', () => {
    const { layout, id } = addSection(EMPTY, 'home', 'Team');

    const account = accountWith([room('!a', { priority: 'Favourite' })], [], {
      sidebar: moveRoom(layout, 'home', '!a', id),
    });

    const shown = sections(account, 'home', NOW);
    expect(ids(shown.find((s) => s.id === FAVOURITES)!.rooms)).toEqual(['!a']);
    expect(shown.find((s) => s.id === id)!.rooms).toEqual([]);
  });

  it('favourites should stay first under an order saved before them', () => {
    const ordered = moveSection(EMPTY, 'home', [ROOMS, DIRECT], DIRECT, 0);
    const account = accountWith([], [], { sidebar: ordered });
    expect(sections(account, 'home', NOW).map((s) => s.id)).toEqual([
      FAVOURITES,
      DIRECT,
      ROOMS,
      LOW_PRIORITY,
    ]);
  });
});

describe('priorityIn', () => {
  it('a room moved into favourites or low priority should take that tag', () => {
    expect([priorityIn(FAVOURITES), priorityIn(LOW_PRIORITY)]).toEqual(['Favourite', 'Low']);
  });

  it('a room moved anywhere else should lose its tag', () => {
    expect([ROOMS, DIRECT, 'section-1'].map(priorityIn)).toEqual(['Normal', 'Normal', 'Normal']);
  });
});

describe('unreadRooms', () => {
  it('rooms mentioning the user should come first', () => {
    const account = accountWith([
      room('!many', { unread: 9 }),
      room('!mention', { unread: 1, mentions: 1 }),
      room('!read'),
    ]);

    expect(ids(unreadRooms(account, 'home'))).toEqual(['!mention', '!many']);
  });
});

describe('directChats', () => {
  it('every direct chat should be listed, latest first, whatever space it is in', () => {
    const account = accountWith(
      [dm('!a', '@a:x'), room('!group'), dm('!b', '@b:x')],
      [spaceSummary('!s', ['!a'])],
      {
        latest: { '!b': { ...message('$b'), timestamp_ms: NOW - 1 } },
      },
    );

    expect(ids(directChats(account, NOW))).toEqual(['!b', '!a']);
  });
});

describe('layout edits', () => {
  it('a new section should come first and get its own ID', () => {
    const first = addSection(EMPTY, 'home', 'Team');
    const second = addSection(first.layout, 'home', 'Friends');
    expect(second.layout.scopes.home!.sections.map((s) => s.name)).toEqual(['Friends', 'Team']);
    expect(second.id).not.toBe(first.id);
  });

  it('a new section should trim its name', () => {
    expect(addSection(EMPTY, 'home', '  Team ').layout.scopes.home!.sections[0]!.name).toBe('Team');
  });

  it('sections should be kept per scope', () => {
    const { layout } = addSection(EMPTY, { space: '!space' }, 'Ops');
    expect(Object.keys(layout.scopes)).toEqual(['!space']);
  });

  it('moving a room should take it out of its old section', () => {
    const a = addSection(EMPTY, 'home', 'A');
    const b = addSection(a.layout, 'home', 'B');
    const moved = moveRoom(moveRoom(b.layout, 'home', '!r', a.id), 'home', '!r', b.id);
    expect(moved.scopes.home!.sections.map((s) => s.rooms.length)).toEqual([1, 0]);
  });

  it('moving a room to a built-in section should free it', () => {
    const a = addSection(EMPTY, 'home', 'A');
    const moved = moveRoom(moveRoom(a.layout, 'home', '!r', a.id), 'home', '!r', ROOMS);
    expect(sectionOf(moved, 'home', '!r')).toBeNull();
  });

  it('a room should land where it is dropped', () => {
    const a = addSection(EMPTY, 'home', 'A');
    let layout = a.layout;

    for (const [id, index] of [
      ['!x', 0],
      ['!y', 1],
      ['!z', 1],
    ] as const) {
      layout = moveRoom(layout, 'home', id, a.id, index);
    }

    expect(layout.scopes.home!.sections[0]!.rooms).toEqual(['!x', '!z', '!y']);
  });

  it('collapsing twice should expand again', () => {
    const twice = toggleCollapsed(toggleCollapsed(EMPTY, 'home', DIRECT), 'home', DIRECT);
    expect(twice.scopes.home!.collapsed).toEqual([]);
  });

  it('deleting a section should forget it', () => {
    const a = addSection(EMPTY, 'home', 'A');
    const deleted = deleteSection(toggleCollapsed(a.layout, 'home', a.id), 'home', a.id);
    expect(deleted.scopes.home).toEqual({ sections: [], collapsed: [], order: [] });
  });

  it('an edit should leave the layout it started from as it was', () => {
    const a = addSection(EMPTY, 'home', 'A');
    moveRoom(a.layout, 'home', '!r', a.id);
    expect(a.layout.scopes.home!.sections[0]!.rooms).toEqual([]);
  });

  it('a space should move to where it is dropped', () => {
    expect(moveSpace(EMPTY, ['!a', '!b', '!c'], '!c', 0).spaces).toEqual(['!c', '!a', '!b']);
  });

  it('a changed layout should be later than the one it changes, even with a clock behind', () => {
    expect(stamped(EMPTY, { ...EMPTY, changed_at: 500 }, 100).changed_at).toBe(501);
  });
});

describe('orderedSpaces', () => {
  it('spaces the order does not name should follow the ones it does', () => {
    const spaces = ['!a', '!b', '!c', '!d'].map((id) => spaceSummary(id));
    const layout = { ...EMPTY, spaces: ['!c', '!a'] };
    expect(orderedSpaces(spaces, layout).map((s) => s.room_id)).toEqual(['!c', '!a', '!b', '!d']);
  });
});
