import type { RoomPreview } from '@better-matrix/matrix-native/types';

import { ADA, roomSummary, spaceSummary, summary } from '../../test/fixtures';
import { joinable, joinedAccounts, joinedAfterPreview, joinedRoute, linkAccount, mayChooseAccount, linkNote, offersJoin, routeLink, shownPreview } from '../linkRoute';
import { parsePermalink } from '../permalinks';
import { apply, initialState } from '../reducer';
import type { State } from '../types';

const HOBBY = '@ada:hobby.chat';
const LAB = '!lab:example.org';
const CLUB = '!club:hobby.chat';

/** Ada's work account (in the lab) and her hobby account (in the club), work on screen. */
function signedIn(): State {
  let state = initialState();
  for (const [id, room] of [
    [ADA, LAB],
    [HOBBY, CLUB],
  ] as const) {
    state = apply(state, { type: 'AccountAdded', data: summary(id) });
    state = apply(state, { type: 'RoomsUpdated', data: { account: id, list: { rooms: [roomSummary(room)], spaces: [], invitations: [] } } });
  }
  return { ...state, active: ADA };
}

function preview(change: Partial<RoomPreview> = {}): RoomPreview {
  return {
    room_id: '!rust:matrix.org',
    alias: '#rust:matrix.org',
    name: 'Rust',
    topic: null,
    avatar: null,
    member_count: 12,
    join_rule: 'Public',
    space: false,
    joined: false,
    invited: false,
    ...change,
  };
}

describe('routeLink', () => {
  it('should call a link that names nothing in Matrix invalid', () => {
    expect(routeLink(signedIn(), parsePermalink('https://example.org'))).toEqual({ type: 'invalid' });
  });

  it('should open a joined room at the linked message', () => {
    const link = parsePermalink(`https://matrix.to/#/${LAB}/$event`);
    expect(routeLink(signedIn(), link)).toEqual({ type: 'room', account: ADA, roomId: LAB, eventId: '$event' });
  });

  it('should open a room only another account joined in that account', () => {
    const link = parsePermalink(`matrix:roomid/${CLUB.slice(1)}`);
    expect(routeLink(signedIn(), link)).toMatchObject({ type: 'room', account: HOBBY, roomId: CLUB });
  });

  it('should send a room it can only know by its alias to the link screen', () => {
    expect(routeLink(signedIn(), parsePermalink('matrix:r/lab:example.org'))).toEqual({ type: 'screen', account: ADA });
  });

  it('should send a person to the link screen for the account the link was tapped in', () => {
    const link = parsePermalink('https://matrix.to/#/@grace:example.org');
    expect(routeLink(signedIn(), link, HOBBY)).toEqual({ type: 'screen', account: HOBBY });
  });

  it('should send a link to the link screen with no account while signed out', () => {
    expect(routeLink(initialState(), parsePermalink(`https://matrix.to/#/${LAB}`))).toEqual({ type: 'screen', account: null });
  });
});

describe('space links', () => {
  const GUILD = '!guild:hobby.chat';
  const inGuild = (): State =>
    apply(signedIn(), {
      type: 'RoomsUpdated',
      data: { account: HOBBY, list: { rooms: [roomSummary(CLUB)], spaces: [spaceSummary(GUILD)], invitations: [] } },
    });

  it('a link to a joined space should show it as its account\'s scope', () => {
    expect(routeLink(inGuild(), parsePermalink(`https://matrix.to/#/${GUILD}`))).toEqual({ type: 'space', account: HOBBY, spaceId: GUILD });
  });

  it('a space just joined should show as a space before the room list knows it', () => {
    expect(joinedRoute(signedIn(), ADA, GUILD, true)).toEqual({ type: 'space', account: ADA, spaceId: GUILD });
  });

  it('a joined room should open as a room', () => {
    expect(joinedRoute(signedIn(), ADA, LAB, false, '$e')).toEqual({ type: 'room', account: ADA, roomId: LAB, eventId: '$e' });
  });
});

describe('linkAccount', () => {
  it('should fall back to the account on screen when the preferred one is signed out', () => {
    expect(linkAccount(signedIn(), '@gone:example.org')).toBe(ADA);
  });
});

describe('joinedAccounts', () => {
  it('should list the preferred account first', () => {
    let state = signedIn();
    state = apply(state, { type: 'RoomsUpdated', data: { account: HOBBY, list: { rooms: [roomSummary(LAB)], spaces: [], invitations: [] } } });
    expect(joinedAccounts(state, LAB, HOBBY)).toEqual([HOBBY, ADA]);
  });
});

describe('joinedAfterPreview', () => {
  it('should open a room the preview says the account joined', () => {
    expect(joinedAfterPreview(signedIn(), preview({ joined: true }), ADA, false)).toBe(ADA);
  });

  it('should open a room another account joined in that account', () => {
    expect(joinedAfterPreview(signedIn(), preview({ room_id: CLUB }), ADA, false)).toBe(HOBBY);
  });

  it('should not leave the account the user picked for another', () => {
    expect(joinedAfterPreview(signedIn(), preview({ room_id: CLUB }), ADA, true)).toBeNull();
  });

  it('should show the preview of a room nobody joined', () => {
    expect(joinedAfterPreview(signedIn(), preview(), ADA, false)).toBeNull();
  });
});

describe('joining', () => {
  it('should offer to join a public room', () => expect(joinable(preview())).toBe(true));

  it('should offer to join an invite-only room the user is invited to', () => {
    expect(joinable(preview({ join_rule: 'Invite', invited: true }))).toBe(true);
  });

  it('should explain an invite-only room instead of offering Join', () => {
    const closed = preview({ join_rule: 'Invite' });
    expect([joinable(closed), linkNote({ type: 'preview', preview: closed })]).toEqual([false, 'link.invite_only']);
  });

  it('should explain a room that takes members who ask', () => {
    expect(linkNote({ type: 'preview', preview: preview({ join_rule: 'Knock' }) })).toBe('link.knock');
  });

  it('should still offer to join a room no server would describe', () => {
    const failed = { type: 'failed', error: 'not found' } as const;
    expect([offersJoin(failed), linkNote(failed)]).toEqual([true, 'link.no_preview']);
  });

  it('should keep showing the preview while joining', () => {
    const before = { type: 'preview', preview: preview() } as const;
    expect(shownPreview({ type: 'joining', before })).toEqual(preview());
  });
});

describe('mayChooseAccount', () => {
  it('the account should not change while a join is under way', () => {
    expect(mayChooseAccount({ type: 'joining', before: { type: 'preview', preview: preview() } })).toBe(false);
  });

  it('the account should not change while a chat is being opened', () => {
    expect(mayChooseAccount({ type: 'person', person: null, loading: false, chatting: true })).toBe(false);
  });

  it('the account should change while a preview shows', () => {
    expect(mayChooseAccount({ type: 'preview', preview: preview() })).toBe(true);
  });
});
