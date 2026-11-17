import type { SpaceSummary } from '@better-matrix/matrix-native/types';

import { ADA, spaceSummary, summary } from '../../test/fixtures';
import { createChoices } from '../create';
import { apply, initialState } from '../reducer';
import type { AccountData, Scope } from '../types';

function accountIn(scope: Scope, spaces: SpaceSummary[] = []): AccountData {
  let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
  state = apply(state, {
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms: [], spaces, invitations: [] } },
  });

  return { ...state.accounts[0]!, scope };
}

describe('createChoices', () => {
  it('home should offer a chat, a room, a space and a section', () =>
    expect(createChoices(accountIn('home'))).toEqual(['chat', 'room', 'space', 'section']));

  it('a space the user may add rooms to should offer a room but no space', () =>
    expect(createChoices(accountIn({ space: '!s' }, [spaceSummary('!s')]))).toEqual([
      'chat',
      'room',
      'section',
    ]));

  it('a space the user may not add rooms to should offer no room', () => {
    const space = { ...spaceSummary('!s'), can_add_rooms: false };
    expect(createChoices(accountIn({ space: '!s' }, [space]))).toEqual(['chat', 'section']);
  });
});
