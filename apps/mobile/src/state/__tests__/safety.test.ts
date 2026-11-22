import type { Invitation, Safety } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, message, roomSummary, summary } from '../../test/fixtures';
import { apply, initialState } from '../reducer';
import {
  DEFAULT_SAFETY,
  hiddenInvitations,
  ignores,
  revealKey,
  shownInvitations,
  showsMedia,
  withChange,
} from '../safety';
import { activeRoomList, mediaHidden } from '../selectors';

const PUBLIC = '!public:x';

const PRIVATE = '!private:x';

function invitation(room_id: string, stranger: boolean): Invitation {
  return {
    room_id,
    name: room_id,
    topic: null,
    avatar: null,
    inviter: null,
    space: false,
    kind: 'Group',
    stranger,
  };
}

const FRIEND = invitation('!friend:x', false);

const STRANGER = invitation('!stranger:x', true);

/** Ada signed in with a public and a private room, two invitations and `safety`. */
function signedIn(safety: Safety = DEFAULT_SAFETY) {
  let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
  state = apply(state, {
    type: 'RoomsUpdated',
    data: {
      account: ADA,
      list: {
        rooms: [{ ...roomSummary(PUBLIC), public: true }, roomSummary(PRIVATE)],
        spaces: [],
        invitations: [FRIEND, STRANGER],
      },
    },
  });

  return apply(state, { type: 'Safety', data: { account: ADA, safety } });
}

describe('safety choices', () => {
  it('media in private rooms only should hide a public room’s media', () => {
    const safety = { ...DEFAULT_SAFETY, media: 'Private' as const };
    expect([showsMedia(safety, true, false), showsMedia(safety, false, false)]).toEqual([
      false,
      true,
    ]);
  });

  it('the user’s own media should always show', () => {
    expect(showsMedia({ ...DEFAULT_SAFETY, media: 'Never' }, true, true)).toBe(true);
  });

  it('ignoring someone twice should list them once, sorted', () => {
    const ignore = (user_id: string) => ({
      type: 'Ignore' as const,
      data: { user_id, ignored: true },
    });

    const safety = [ignore('@zed:x'), ignore('@amy:x'), ignore('@amy:x')].reduce(
      withChange,
      DEFAULT_SAFETY,
    );

    expect(safety.ignored).toEqual(['@amy:x', '@zed:x']);
  });

  it('stopping ignoring someone should take them off the list', () => {
    const safety = withChange(
      { ...DEFAULT_SAFETY, ignored: ['@amy:x'] },
      { type: 'Ignore', data: { user_id: '@amy:x', ignored: false } },
    );

    expect(ignores(safety, '@amy:x')).toBe(false);
  });

  it('hiding strangers’ invitations should list only the others', () => {
    const safety = { ...DEFAULT_SAFETY, hide_stranger_invites: true };
    expect([
      shownInvitations([FRIEND, STRANGER], safety),
      hiddenInvitations([FRIEND, STRANGER], safety),
    ]).toEqual([[FRIEND], [STRANGER]]);
  });

  it('the same invitations should give the same shown list each time', () => {
    const safety = { ...DEFAULT_SAFETY, hide_stranger_invites: true };
    const list = [FRIEND, STRANGER];
    expect(shownInvitations(list, safety)).toBe(shownInvitations(list, { ...safety }));
  });

  it('showing strangers’ invitations should keep the list as it is', () => {
    const list = [FRIEND, STRANGER];
    expect(shownInvitations(list, DEFAULT_SAFETY)).toBe(list);
  });
});

describe('safety in the state', () => {
  it('an account’s reported choices should be kept', () => {
    const safety = { ...DEFAULT_SAFETY, ignored: [GRACE] };
    expect(signedIn(safety).accounts[0]!.safety).toEqual(safety);
  });

  it('a new account should show everything until its choices are reported', () => {
    const state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
    expect(state.accounts[0]!.safety).toEqual(DEFAULT_SAFETY);
  });

  it('the room list should leave out strangers’ invitations the user hides', () => {
    const state = signedIn({ ...DEFAULT_SAFETY, hide_stranger_invites: true });
    expect(activeRoomList(state)!.invitations).toEqual([FRIEND]);
  });

  it('someone else’s media in a public room should wait when media shows in private rooms only', () => {
    const state = signedIn({ ...DEFAULT_SAFETY, media: 'Private' });
    expect([
      mediaHidden(state, ADA, PUBLIC, message('$a')),
      mediaHidden(state, ADA, PRIVATE, message('$a')),
    ]).toEqual([true, false]);
  });

  it('media the user showed should stay shown', () => {
    const state = signedIn({ ...DEFAULT_SAFETY, media: 'Never' });
    const shown = { ...state, revealedMedia: { [revealKey(ADA, '$a')]: true as const } };
    expect(mediaHidden(shown, ADA, PUBLIC, message('$a'))).toBe(false);
  });
});
