import type { ImagePack } from '@better-matrix/matrix-native/types';

import { emotePill, EVERYONE, triggerAt } from '../pills';
import { apply, initialState } from '../reducer';
import { packImages, suggestionsFor } from '../suggestions';
import type { State } from '../types';
import { ADA, GRACE, roomSummary, summary } from '../../test/fixtures';

const ROOM = '!r:x';

function pack(id: string, codes: string[], sticker = false): ImagePack {
  return {
    id,
    name: id,
    avatar: null,
    avatar_url: null,
    place: { type: 'User' },
    editable: true,
    everywhere: false,
    images: codes.map((code) => ({
      shortcode: code,
      body: code,
      url: `mxc://x/${code}`,
      media: `m-${code}`,
      emoticon: !sticker,
      sticker,
      dimensions: null,
      mimetype: null,
    })),
  };
}

function state(kind: 'Group' | 'Direct' = 'Group'): State {
  const events: Parameters<typeof apply>[1][] = [
    { type: 'AccountAdded', data: summary(ADA) },
    {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: {
          rooms: [{ ...roomSummary(ROOM, 'general'), kind }, roomSummary('!d:x', 'design')],
          spaces: [],
          invitations: [],
        },
      },
    },
    {
      type: 'Members',
      data: {
        request: null,
        account: ADA,
        room_id: ROOM,
        members: {
          people: [ADA, GRACE].map((id) => ({
            person: { id, name: id === ADA ? 'Ada' : 'Grace Hopper', avatar: null },
            role: 'Member' as const,
            presence: 'Online' as const,
            status_message: null,
            verified: false,
            level: 0,
          })),
          complete: true,
          can_invite: true,
          own_level: 0,
          can_kick: false,
          can_ban: false,
          can_set_roles: false,
          knocking: [],
          banned: [],
        },
      },
    },
    {
      type: 'ImagePacks',
      data: {
        account: ADA,
        room_id: ROOM,
        packs: [pack('user', ['party']), pack('room', ['panda'])],
        may_add: false,
      },
    },
  ];

  return events.reduce((s, e) => apply(s, e), initialState());
}

const suggest = (draft: string, s = state()) =>
  suggestionsFor(s, ADA, ROOM, triggerAt(draft, draft.length)!);

describe('suggestionsFor', () => {
  it('should suggest the people in the room but the user, then @room', () => {
    expect(suggest('@').map((s) => s.key)).toEqual([`user:${GRACE}`, 'everyone']);
  });

  it('should suggest people by any word of their name', () => {
    expect(suggest('@hop').map((s) => s.key)).toEqual([`user:${GRACE}`]);
  });

  it('should not suggest @room in a direct chat', () => {
    expect(suggest('@ro', state('Direct'))).toEqual([]);
  });

  it('should insert @room as the pill that mentions the room', () => {
    expect(suggest('@room')).toMatchObject([{ pill: EVERYONE }]);
  });

  it('should suggest joined rooms for #', () => {
    expect(suggest('#des').map((s) => s.key)).toEqual(['room:!d:x']);
  });

  it('should suggest custom emoji before standard ones', () => {
    expect(suggest(':pa').slice(0, 2)).toMatchObject([
      { type: 'emote', pill: emotePill('mxc://x/party', 'party') },
      { type: 'emote', pill: emotePill('mxc://x/panda', 'panda') },
    ]);
  });

  it('should suggest commands for a slash', () => {
    expect(suggest('/sp').map((s) => s.key)).toEqual(['command:spoiler']);
  });
});

describe('packImages', () => {
  it('should list each image of a use once', () => {
    const packs = [pack('a', ['x', 'y']), pack('b', ['x']), pack('c', ['s'], true)];
    expect(packImages(packs, 'emoticon').map(({ image }) => image.shortcode)).toEqual(['x', 'y']);
  });
});

describe('/meet among the suggestions', () => {
  const meet = { kind: 'command' as const, query: 'mee', start: 0, end: 4 };

  const startable = (can: boolean) =>
    apply(state(), {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: {
          rooms: [{ ...roomSummary(ROOM, 'general'), can_start_meeting: can }],
          spaces: [],
          invitations: [],
        },
      },
    });

  it('should suggest /meet where the room can start a meeting', () => {
    expect(suggestionsFor(startable(true), ADA, ROOM, meet).map((s) => s.key)).toEqual([
      'command:meet',
    ]);
  });

  it('should not suggest /meet where the room can’t', () => {
    expect(suggestionsFor(startable(false), ADA, ROOM, meet)).toEqual([]);
  });
});
