import type { Members, Person, RoomMember } from '@better-matrix/matrix-native/types';

import {
  EMPTY_SEARCH,
  type PeopleSearch,
  asksFirst,
  busy,
  candidates,
  createRoomCommand,
  createSpaceCommand,
  dayOf,
  due,
  fileOpens,
  filterMembers,
  finished,
  found,
  inviteTargets,
  isUserId,
  matches,
  may,
  mayRename,
  renameCommand,
  rolesOffered,
  searching,
  spacesForNewRoom,
  typed,
  withNotificationMode,
} from '../people';
import { ADA, GRACE, roomSummary } from '../../test/fixtures';

function person(id: string, name = id): Person {
  return { id, name, avatar: null };
}

function member(
  id: string,
  name: string,
  role: RoomMember['role'] = 'Member',
  level = 0,
): RoomMember {
  return { person: person(id, name), role, presence: 'Unknown', verified: false, level };
}

function members(own_level: number, rights: Partial<Members> = {}): Members {
  return {
    people: [],
    complete: true,
    can_invite: true,
    own_level,
    can_kick: true,
    can_ban: true,
    can_set_roles: true,
    ...rights,
  };
}

const space = (room_id: string, can_add_rooms: boolean) => ({
  room_id,
  name: room_id,
  avatar: null,
  can_add_rooms,
  can_rename: false,
  children: [],
  members: [],
});

describe('matches', () => {
  it('should match a query against the starts of words', () => {
    expect(matches('Grace Hopper', 'gr ho')).toBe(true);
  });

  it('should not match the middle of a word', () => {
    expect(matches('Grace Hopper', 'race')).toBe(false);
  });

  it('should match words after punctuation', () => {
    expect(matches('#release-notes', 'notes')).toBe(true);
  });
});

describe('filterMembers', () => {
  const people = [
    member('@ada:example.org', 'Ada Lovelace'),
    member('@grace:navy.mil', 'Grace Hopper'),
  ];

  it('should keep everyone in order when nothing is typed', () => {
    expect(filterMembers(people, '  ')).toBe(people);
  });

  it('should find members by name', () => {
    expect(filterMembers(people, 'hop').map((m) => m.person.name)).toEqual(['Grace Hopper']);
  });

  it('should find members by any part of their user ID', () => {
    expect(filterMembers(people, 'navy').map((m) => m.person.name)).toEqual(['Grace Hopper']);
  });
});

describe('moderation rights', () => {
  it('should let a moderator with the right kick a member below them', () => {
    expect(may(members(50), { type: 'Kick' }, member(GRACE, 'Grace'))).toBe(true);
  });

  it('should not let anyone act on a member at their own level', () => {
    expect(may(members(50), { type: 'Ban' }, member(GRACE, 'Grace', 'Moderator', 50))).toBe(false);
  });

  it('should not let a moderator without the right ban', () => {
    expect(may(members(50, { can_ban: false }), { type: 'Ban' }, member(GRACE, 'Grace'))).toBe(
      false,
    );
  });

  it('should offer only the roles up to the user own level, besides the member current one', () => {
    expect(rolesOffered(members(50), member(GRACE, 'Grace'))).toEqual(['Moderator']);
  });

  it('should offer no roles without the right to set them', () => {
    expect(rolesOffered(members(100, { can_set_roles: false }), member(GRACE, 'Grace'))).toEqual(
      [],
    );
  });

  it('should ask before kicking or banning', () => {
    expect([asksFirst({ type: 'Kick' }, 100), asksFirst({ type: 'Ban' }, 100)]).toEqual([
      true,
      true,
    ]);
  });

  it('should give a role below the user own level without asking', () => {
    expect(asksFirst({ type: 'Role', data: 'Moderator' }, 100)).toBe(false);
  });

  it('should ask before giving the user own role, which cannot be taken back', () => {
    expect(asksFirst({ type: 'Role', data: 'Admin' }, 100)).toBe(true);
  });
});

describe('rooms', () => {
  it('should let a moderator rename a group room', () => {
    expect(mayRename(roomSummary('!a'), members(50))).toBe(true);
  });

  it('should not let a member rename a room', () => {
    expect(mayRename(roomSummary('!a'), members(0))).toBe(false);
  });

  it('should not offer renaming a direct chat', () => {
    expect(mayRename(roomSummary('!a', 'Grace', 'Direct'), members(100))).toBe(false);
  });

  it('should offer the other group rooms to invite a member to', () => {
    const rooms = [
      roomSummary('!here'),
      roomSummary('!there'),
      roomSummary('!dm', 'Grace', 'Direct'),
    ];

    expect(inviteTargets(rooms, '!here').map((r) => r.room_id)).toEqual(['!there']);
  });

  it('should offer only the spaces the user may add rooms to', () => {
    expect(spacesForNewRoom([space('!a', true), space('!b', false)]).map((s) => s.room_id)).toEqual(
      ['!a'],
    );
  });

  it('should not make a room without a name', () => {
    expect(createRoomCommand(ADA, { name: '  ', topic: 'x', space: null })).toBeNull();
  });

  it('should make a room with its trimmed name, its topic and its space', () => {
    expect(createRoomCommand(ADA, { name: ' Lab ', topic: ' Notes ', space: '!s' })).toEqual({
      type: 'CreateRoom',
      data: { account: ADA, name: 'Lab', topic: 'Notes', space: '!s' },
    });
  });

  it('should make a room without a topic when none is typed', () => {
    expect(createRoomCommand(ADA, { name: 'Lab', topic: ' ', space: null })?.data.topic).toBeNull();
  });

  it('should not make a space without a name', () => {
    expect(createSpaceCommand(ADA, '')).toBeNull();
  });

  it('should not rename a room to the name it has', () => {
    expect(renameCommand(ADA, '!a', 'Lab', ' Lab ')).toBeNull();
  });

  it('should rename a room to the trimmed new name', () => {
    expect(renameCommand(ADA, '!a', 'Lab', ' Studio ')).toEqual({
      type: 'RenameRoom',
      data: { account: ADA, room_id: '!a', name: 'Studio' },
    });
  });

  it('should make a room follow the account default when its own mode is cleared', () => {
    const room = {
      ...roomSummary('!a'),
      notifications: 'Mute' as const,
      custom_notifications: true,
    };

    expect(withNotificationMode(room, null, 'MentionsOnly')).toMatchObject({
      notifications: 'MentionsOnly',
      custom_notifications: false,
    });
  });

  it('should open images and videos in the preview and share other files', () => {
    expect([fileOpens('Image'), fileOpens('Video'), fileOpens('Audio'), fileOpens('File')]).toEqual(
      ['preview', 'preview', 'share', 'share'],
    );
  });
});

describe('people search', () => {
  /** A search that typed `text` and sent it as `request`. */
  function sent(text: string, request: number, from: PeopleSearch = EMPTY_SEARCH): PeopleSearch {
    const next = typed(from, text);

    return searching(next, due(next) ?? '', request);
  }

  it('should search a changed query', () => {
    expect(due(typed(EMPTY_SEARCH, ' ada '))).toBe('ada');
  });

  it('should not search a query again once sent', () => {
    expect(due(sent('ada', 1))).toBeNull();
  });

  it('should keep the answer to the latest search', () => {
    expect(found(sent('ada', 1), 1, [person(ADA)]).people).toEqual([person(ADA)]);
  });

  it('should drop the answer to an older search', () => {
    const search = sent('grace', 2, sent('ada', 1));
    expect(found(search, 1, [person(ADA)]).people).toEqual([]);
  });

  it('should drop an answer arriving after the text changed', () => {
    const search = typed(sent('ada', 1), 'adam');
    expect(found(search, 1, [person(ADA)]).people).toEqual([]);
  });

  it('should search again a query whose answer was dropped', () => {
    const search = typed(typed(sent('ada', 1), 'ad'), 'ada');
    expect(due(search)).toBe('ada');
  });

  it('should clear the results when the text is cleared', () => {
    const search = typed(found(sent('ada', 1), 1, [person(ADA)]), '');
    expect(search.people).toEqual([]);
  });

  it('should list the suggestions while nothing is typed', () => {
    const search = found({ ...EMPTY_SEARCH, suggesting: 7 }, 7, [person(GRACE)]);
    expect(candidates(search, ADA)).toEqual([person(GRACE)]);
  });

  it('should keep listing the last results while new text rests', () => {
    const search = typed(found(sent('ada', 1), 1, [person(GRACE)]), 'gr');
    expect(candidates(search, ADA)).toEqual([person(GRACE)]);
  });

  it('should never list the user', () => {
    const search = found(sent('a', 1), 1, [person(ADA), person(GRACE)]);
    expect(candidates(search, ADA)).toEqual([person(GRACE)]);
  });

  it('should offer a typed user ID the directory does not know', () => {
    expect(candidates(sent('@turing:example.org', 1), ADA)).toEqual([
      person('@turing:example.org'),
    ]);
  });

  it('should not offer a typed user ID twice when the directory found it', () => {
    const search = found(sent(GRACE, 1), 1, [person(GRACE, 'Grace')]);
    expect(candidates(search, ADA)).toEqual([person(GRACE, 'Grace')]);
  });

  it('should tell a full user ID from a name', () => {
    expect([isUserId('@ada:example.org'), isUserId('ada'), isUserId('@ada')]).toEqual([
      true,
      false,
      false,
    ]);
  });

  it('should be busy while the typed text is not answered', () => {
    expect(busy(typed(EMPTY_SEARCH, 'ada'))).toBe(true);
  });

  it('should not be busy once the search finished', () => {
    expect(busy(finished(sent('ada', 1), 1))).toBe(false);
  });
});

describe('dayOf', () => {
  const now = new Date(2026, 10, 16, 12, 0).getTime();

  it('should call a time earlier the same day today', () => {
    expect(dayOf(new Date(2026, 10, 16, 0, 5).getTime(), now)).toBe('today');
  });

  it('should call a time the day before yesterday', () => {
    expect(dayOf(new Date(2026, 10, 15, 23, 59).getTime(), now)).toBe('yesterday');
  });

  it('should give the date of anything older', () => {
    expect(dayOf(new Date(2026, 0, 3, 9, 0).getTime(), now)).toEqual({
      year: 2026,
      month: 1,
      day: 3,
    });
  });
});
