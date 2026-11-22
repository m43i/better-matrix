import type { RoomSummary } from '@better-matrix/matrix-native/types';

import {
  ADA,
  GRACE,
  message,
  person,
  roomSummary,
  spaceSummary,
  summary,
  withContent,
} from '../../test/fixtures';
import { foundAny, jumpTo, listTime, messageText, previewLine, searchResults } from '../lists';
import { apply, initialState } from '../reducer';
import type { AccountData, State } from '../types';

const NOW = new Date(2026, 10, 16, 15, 0).getTime();

function signedIn(...ids: string[]): State {
  return ids.reduce(
    (s, id) => apply(s, { type: 'AccountAdded', data: summary(id, id === ADA ? 'Ada' : 'Grace') }),
    initialState(),
  );
}

function accountWith(
  rooms: RoomSummary[],
  spaces = [spaceSummary('!s', [], [], 'Design')],
): AccountData {
  const state = apply(signedIn(ADA), {
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms, spaces, invitations: [] } },
  });

  return state.accounts[0]!;
}

describe('messageText', () => {
  it('a text message should read as one line', () => {
    expect(messageText(message('$m', 'hello\n\nthere'))).toBe('hello there');
  });

  it('an attachment should read as its file name', () => {
    const content = {
      type: 'Attachment' as const,
      data: {
        kind: 'File' as const,
        name: 'plan.pdf',
        size: null,
        media: null,
        thumbnail: null,
        dimensions: null,
        audio: null,
      },
    };

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
    expect(jumpTo(accountWith(rooms), 'design', NOW).rooms.map((r) => r.room_id)).toEqual([
      '!design',
      '!design-chat',
    ]);
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
    const account = {
      ...accountWith(rooms),
      latest: { '!other': { ...message('$o'), timestamp_ms: NOW - 1 } },
    };

    expect(jumpTo(account, ' ', NOW).rooms[0]!.room_id).toBe('!other');
  });
});

describe('searchResults', () => {
  it('a pasted address should be offered as a link', () => {
    const results = searchResults(accountWith([]), ' #rust:matrix.org ', [], NOW);
    expect(results.link?.id).toBe('#rust:matrix.org');
  });

  it('a name should not be read as a link', () =>
    expect(searchResults(accountWith([]), 'rust', [], NOW).link).toBeNull());

  it('the user should not be among the people found', () => {
    const found = [person(ADA, 'Ada'), person(GRACE, 'Grace')];
    expect(searchResults(accountWith([]), 'a', found, NOW).people.map((p) => p.id)).toEqual([
      GRACE,
    ]);
  });

  it('a term matching nothing should find nothing', () =>
    expect(foundAny(searchResults(accountWith([]), 'zzz', [], NOW))).toBe(false));
});

describe('search interaction ranking', () => {
  it('should put a frequently used matching room before a closer unused name', () => {
    const account = accountWith([
      roomSummary('!new', 'General'),
      roomSummary('!used', 'General archive'),
    ]);

    account.searchUsage = { rooms: { '!used': 10 }, people: {} };
    expect(searchResults(account, 'general', [], NOW).rooms.map((r) => r.room_id)).toEqual([
      '!used',
      '!new',
    ]);
  });

  it('should rank frequently contacted people before the directory order', () => {
    const account = accountWith([]);
    account.searchUsage = { rooms: {}, people: { [GRACE]: 12 } };
    expect(
      searchResults(
        account,
        'a',
        [person('@alan:example.org', 'Alan'), person(GRACE, 'Grace')],
        NOW,
      ).people.map((p) => p.id),
    ).toEqual([GRACE, '@alan:example.org']);
  });

  it('should keep an explicitly typed user ahead of frequent contacts', () => {
    const account = accountWith([]);
    account.searchUsage = { rooms: {}, people: { [GRACE]: 12 } };
    expect(
      searchResults(
        account,
        '@alan:example.org',
        [person(GRACE, 'Grace'), person('@alan:example.org', 'Alan')],
        NOW,
      ).people[0]!.id,
    ).toBe('@alan:example.org');
  });

  it('should prefer messages from frequently used rooms or frequently contacted senders', () => {
    const account = accountWith([roomSummary('!new'), roomSummary('!used')]);
    account.searchUsage = { rooms: { '!used': 10 }, people: { [GRACE]: 20 } };

    const hit = {
      room_id: '!new',
      event_id: '$new',
      thread: null,
      sender: '@alan:example.org',
      timestamp_ms: NOW,
      body: 'release',
      highlights: [0],
    };

    const hits = [
      hit,
      { ...hit, room_id: '!used', event_id: '$room' },
      { ...hit, sender: GRACE, event_id: '$person' },
    ];

    expect(
      searchResults(account, 'release', [], NOW, hits).messages.map((h) => h.event_id),
    ).toEqual(['$person', '$room', '$new']);
  });

  it('should leave out message hits from people the user ignores', () => {
    const account = accountWith([roomSummary('!room')]);
    account.safety = { ...account.safety, ignored: [GRACE] };

    const hit = {
      room_id: '!room',
      event_id: '$alan',
      thread: null,
      sender: '@alan:example.org',
      timestamp_ms: NOW,
      body: 'release',
      highlights: [0],
    };

    const hits = [hit, { ...hit, sender: GRACE, event_id: '$grace' }];
    expect(
      searchResults(account, 'release', [], NOW, hits).messages.map((h) => h.event_id),
    ).toEqual(['$alan']);
  });
});

it('search usage events should only update their own account', () => {
  const state = signedIn(ADA, GRACE);

  const next = apply(state, {
    type: 'SearchUsage',
    data: { account: ADA, usage: { rooms: { '!room': 8 }, people: {} } },
  });

  expect(next.accounts.map((a) => a.searchUsage.rooms['!room'] ?? 0)).toEqual([8, 0]);
});
