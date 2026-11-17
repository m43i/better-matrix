import type { Members, ReadReceipt } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, messageEntry, person, summary } from '../../test/fixtures';
import { readerOf, readersOf } from '../readers';
import { apply, initialState } from '../reducer';
import { account as accountOf } from '../selectors';
import type { State } from '../types';

const ROOM = '!lab:example.org';

const KAI = '@kai:example.org';

const SAM = '@sam:example.org';

const receipt = (user_id: string): ReadReceipt => ({ user_id, timestamp_ms: 1_700_000_060_000 });

/** Ada's account with #lab open: Grace's message read by Kai and Sam, one being sent. */
function setUp(): State {
  let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
  state = apply(state, {
    type: 'TimelineChanged',
    data: {
      account: ADA,
      key: { room_id: ROOM, thread: null },
      diffs: [
        {
          type: 'Reset',
          data: [
            messageEntry('$read', { read_receipts: [receipt(KAI), receipt(SAM)] }),
            messageEntry('$sending', { pending: true, read_receipts: [receipt(KAI)] }),
          ],
        },
      ],
    },
  });

  return state;
}

function withMembers(state: State): State {
  const members: Members = {
    people: [
      {
        person: person(KAI, 'Kai'),
        role: 'Member',
        presence: 'Online',
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
  };

  return apply(state, {
    type: 'Members',
    data: { request: null, account: ADA, room_id: ROOM, members },
  });
}

describe('readers', () => {
  it('should list who read up to a message', () => {
    const readers = readersOf(setUp(), ADA, ROOM, '$read').map((r) => r.user_id);
    expect(readers).toEqual([KAI, SAM]);
  });

  it('should list nobody for a message still being sent', () => {
    expect(readersOf(setUp(), ADA, ROOM, '$sending')).toEqual([]);
  });

  it('should list nobody for a message the timeline does not hold', () => {
    expect(readersOf(setUp(), ADA, ROOM, '$elsewhere')).toEqual([]);
  });

  it('should name a reader as the room’s members do', () => {
    const state = withMembers(setUp());
    expect(readerOf(accountOf(state, ADA), ROOM, KAI).name).toBe('Kai');
  });

  it('should name a reader who is no loaded member as their messages do', () => {
    expect(readerOf(accountOf(setUp(), ADA), ROOM, GRACE).name).toBe('Grace');
  });

  it('should name a reader the room knows nothing about by their user ID', () => {
    expect(readerOf(accountOf(setUp(), ADA), ROOM, SAM)).toEqual(person(SAM));
  });
});
