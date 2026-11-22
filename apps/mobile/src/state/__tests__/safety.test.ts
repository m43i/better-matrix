import type { Attachment, Invitation, Safety } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, message, messageEntry, roomSummary, summary } from '../../test/fixtures';
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
import { lastMessage } from '../sections';
import {
  activeRoomList,
  galleryThumbnail,
  mediaHidden,
  roomPreviewPicture,
  shownFiles,
  previewHidden,
  pinnedPreviews,
  previewItems,
  previewMessage,
  savedMessages,
} from '../selectors';
import { roomKey, timelineId } from '../timeline';

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

describe('ignoring someone', () => {
  const ignoreGrace = (state: ReturnType<typeof signedIn>) =>
    apply(state, {
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, ignored: [GRACE] } },
    });

  it('a room whose newest message is from someone the user then ignores should lose its preview', () => {
    const state = apply(signedIn(), {
      type: 'LatestMessage',
      data: { account: ADA, room_id: PRIVATE, message: message('$spam') },
    });

    expect(lastMessage(ignoreGrace(state).accounts[0]!, PRIVATE)).toBeUndefined();
  });

  it('their messages in a kept timeline should draw nothing, keeping its length', () => {
    const state = apply(signedIn(), {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: roomKey(PRIVATE),
        diffs: [{ type: 'Reset', data: [messageEntry('$spam')] }],
      },
    });

    const entries =
      ignoreGrace(state).accounts[0]!.timelines[timelineId(roomKey(PRIVATE))]!.entries;

    expect(entries).toEqual([{ id: '$spam', kind: { type: 'Other' } }]);
  });

  it('choices that ignore no one new should leave previews alone', () => {
    const state = apply(signedIn(), {
      type: 'LatestMessage',
      data: { account: ADA, room_id: PRIVATE, message: message('$hi') },
    });

    const after = apply(state, {
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, media: 'Never' } },
    });

    expect(after.accounts[0]!.latest).toBe(state.accounts[0]!.latest);
  });
});

describe('the attachment preview', () => {
  const image: Attachment = {
    kind: 'Image',
    name: 'photo.png',
    size: 1000,
    media: 'mxc://example.org/photo',
    thumbnail: null,
    dimensions: null,
    audio: null,
  };

  /** Grace posted two pictures together in the public room; media shows in private rooms only. */
  function batch() {
    const state = signedIn({ ...DEFAULT_SAFETY, media: 'Private' });
    const content = { type: 'Attachment' as const, data: image };

    return apply(state, {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: roomKey(PUBLIC),
        diffs: [
          {
            type: 'Reset',
            data: [
              messageEntry('$one', { content, timestamp_ms: 1 }),
              messageEntry('$two', { content, timestamp_ms: 2 }),
            ],
          },
        ],
      },
    });
  }

  it('should leave hidden media of the batch out of the gallery', () => {
    const items = previewItems(batch(), ADA, roomKey(PUBLIC), '$one', '$one');
    expect(items.map((m) => m.event_id)).toEqual(['$one']);
  });

  it('should keep the opened item hidden until the user shows it, from wherever it opened', () => {
    const state = batch();
    const [opened] = previewItems(state, ADA, roomKey(PUBLIC), '$one', '$one');
    expect(mediaHidden(state, ADA, PUBLIC, opened!)).toBe(true);
  });

  it('should step through the batch once its media is shown', () => {
    const state = {
      ...batch(),
      revealedMedia: { [revealKey(ADA, '$two')]: true as const },
    };

    expect(
      previewItems(state, ADA, roomKey(PUBLIC), '$one', '$one').map((m) => m.event_id),
    ).toEqual(['$one', '$two']);
  });
});

describe('the preview’s thumbnails and listed files', () => {
  const picture = (name: string): Attachment => ({
    kind: 'Image',
    name,
    size: 1000,
    media: `mxc://example.org/${name}`,
    thumbnail: null,
    dimensions: null,
    audio: null,
  });

  it('a hidden item should fetch no thumbnail while a shown sibling does', () => {
    const state = {
      ...signedIn({ ...DEFAULT_SAFETY, media: 'Never' }),
      revealedMedia: { [revealKey(ADA, '$two')]: true as const },
    };

    const one = messageEntry('$one', { content: { type: 'Attachment', data: picture('one') } });
    const two = messageEntry('$two', { content: { type: 'Attachment', data: picture('two') } });

    const thumbs = [one, two].map((e) =>
      e.kind.type === 'Message' ? galleryThumbnail(state, ADA, PRIVATE, e.kind.data) : 'none',
    );

    expect(thumbs).toEqual([null, 'mxc://example.org/two']);
  });

  /** Room Info listed a file from Grace that no timeline holds. */
  function listed(attachment: Attachment) {
    const state = signedIn({ ...DEFAULT_SAFETY, media: 'Never' });

    const file = {
      event_id: '$file',
      sender_id: GRACE,
      sender_name: 'Grace',
      timestamp: 1,
      attachment,
    };

    return apply(state, { type: 'Files', data: { account: ADA, room_id: PRIVATE, files: [file] } });
  }

  it('a listed file no timeline holds should still open, kept back as someone else’s', () => {
    const state = listed(picture('photo'));
    const items = previewItems(state, ADA, roomKey(PRIVATE), '$file', '$file');
    expect([items.map((m) => m.event_id), previewHidden(state, ADA, PRIVATE, items[0]!)]).toEqual([
      ['$file'],
      true,
    ]);
  });

  it('an audio clip should never wait for Show', () => {
    const state = listed({ ...picture('clip'), kind: 'Audio' });
    const [clip] = previewItems(state, ADA, roomKey(PRIVATE), '$file', '$file');
    expect(previewHidden(state, ADA, PRIVATE, clip!)).toBe(false);
  });
});

describe('a listed file whose message a timeline holds', () => {
  const photo: Attachment = {
    kind: 'Image',
    name: 'photo.png',
    size: 1000,
    media: 'mxc://example.org/photo',
    thumbnail: null,
    dimensions: null,
    audio: null,
  };

  /** Room Info listed Grace's photo `$file`; the room's timeline holds `$file` as `entries`. */
  function heldAs(state: ReturnType<typeof signedIn>, entries: ReturnType<typeof messageEntry>[]) {
    const file = {
      event_id: '$file',
      sender_id: GRACE,
      sender_name: 'Grace',
      timestamp: 1,
      attachment: photo,
    };

    const listed = apply(state, {
      type: 'Files',
      data: { account: ADA, room_id: PRIVATE, files: [file] },
    });

    return apply(listed, {
      type: 'TimelineChanged',
      data: { account: ADA, key: roomKey(PRIVATE), diffs: [{ type: 'Reset', data: entries }] },
    });
  }

  it('a deleted message should not come back from the file list', () => {
    const state = heldAs(signedIn(), [messageEntry('$file', { content: { type: 'Redacted' } })]);
    expect(previewItems(state, ADA, roomKey(PRIVATE), '$file', '$file')).toEqual([]);
  });

  it('a message hidden as its sender is ignored should not come back from the file list', () => {
    const state = heldAs(signedIn({ ...DEFAULT_SAFETY, ignored: [GRACE] }), [
      messageEntry('$file', { content: { type: 'Attachment', data: photo } }),
    ]);

    expect(previewItems(state, ADA, roomKey(PRIVATE), '$file', '$file')).toEqual([]);
  });
});

describe('a previewed room the account is invited to', () => {
  const preview = {
    room_id: '!spam:x',
    alias: null,
    name: 'Free crypto',
    topic: null,
    avatar: 'mxc://spam.example/logo',
    member_count: 3,
    join_rule: 'Invite' as const,
    space: false,
    joined: false,
    invited: true,
  };

  it('should show initials while pictures in invitations are hidden', () => {
    const state = signedIn({ ...DEFAULT_SAFETY, invite_pictures: false });
    expect(roomPreviewPicture(state, ADA, preview)).toBeNull();
  });

  it('should show its picture while pictures in invitations show', () => {
    expect(roomPreviewPicture(signedIn(), ADA, preview)).toBe(preview.avatar);
  });

  it('should show a room’s picture the account isn’t invited to', () => {
    const state = signedIn({ ...DEFAULT_SAFETY, invite_pictures: false });
    expect(roomPreviewPicture(state, ADA, { ...preview, invited: false })).toBe(preview.avatar);
  });
});

describe('Room Info’s files', () => {
  const photo: Attachment = {
    kind: 'Image',
    name: 'photo.png',
    size: 1000,
    media: 'mxc://example.org/photo',
    thumbnail: null,
    dimensions: null,
    audio: null,
  };

  const file = (event_id: string, sender_id: string) => ({
    event_id,
    sender_id,
    sender_name: sender_id,
    timestamp: 1,
    attachment: photo,
  });

  /** Room Info listed `files` for the private room; media never shows at once. */
  function listing(files: ReturnType<typeof file>[], safety: Partial<Safety> = {}) {
    const state = signedIn({ ...DEFAULT_SAFETY, media: 'Never', ...safety });

    return apply(state, { type: 'Files', data: { account: ADA, room_id: PRIVATE, files } });
  }

  it('should leave out what people the user ignores shared', () => {
    const state = listing([file('$a', GRACE), file('$b', ADA)], { ignored: [GRACE] });
    expect(shownFiles(state, ADA, PRIVATE)?.map((f) => f.event_id)).toEqual(['$b']);
  });

  it('the user’s own listed file should show at once', () => {
    const state = listing([file('$mine', ADA)]);
    const [mine] = previewItems(state, ADA, roomKey(PRIVATE), '$mine', '$mine');
    expect(previewHidden(state, ADA, PRIVATE, mine!)).toBe(false);
  });

  it('an ignored person’s listed file should never be rebuilt', () => {
    const state = listing([file('$a', GRACE)], { ignored: [GRACE] });
    expect(previewItems(state, ADA, roomKey(PRIVATE), '$a', '$a')).toEqual([]);
  });

  it('an attachment held only in a thread should open', () => {
    const state = apply(listing([]), {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: { room_id: PRIVATE, thread: '$root' },
        diffs: [
          {
            type: 'Reset',
            data: [messageEntry('$shot', { content: { type: 'Attachment', data: photo } })],
          },
        ],
      },
    });

    const items = previewItems(state, ADA, roomKey(PRIVATE), '$shot', '$shot');
    expect(items.map((m) => m.event_id)).toEqual(['$shot']);
  });

  it('a message deleted in its timeline should leave the listed files', () => {
    const state = apply(listing([file('$a', GRACE), file('$b', GRACE)]), {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: roomKey(PRIVATE),
        diffs: [{ type: 'PushBack', data: messageEntry('$a', { content: { type: 'Redacted' } }) }],
      },
    });

    expect(state.accounts[0]!.files[PRIVATE]?.map((f) => f.event_id)).toEqual(['$b']);
  });
});

it('a previewed room held as an invitation should hide its picture, whatever the preview says', () => {
  const state = signedIn({ ...DEFAULT_SAFETY, invite_pictures: false });

  const preview = {
    room_id: STRANGER.room_id,
    alias: null,
    name: 'Stranger',
    topic: null,
    avatar: 'mxc://spam.example/logo',
    member_count: 3,
    join_rule: 'Invite' as const,
    space: false,
    joined: false,
    invited: false,
  };

  expect(roomPreviewPicture(state, ADA, preview)).toBeNull();
});

describe('round five', () => {
  const photo: Attachment = {
    kind: 'Image',
    name: 'photo.png',
    size: 1000,
    media: 'mxc://example.org/photo',
    thumbnail: null,
    dimensions: null,
    audio: null,
  };

  const preview = (event_id: string, sender_id: string) => ({
    event_id,
    sender_name: sender_id,
    sender_id,
    sender_avatar: null,
    timestamp_ms: 1,
    body: 'hi',
    thread: null,
  });

  /** The private room pins and saves `$a` (Grace's) and `$b` (Ada's). */
  function pinnedAndSaved(safety: Safety) {
    let state = signedIn(safety);
    state = apply(state, {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: {
          rooms: [{ ...roomSummary(PRIVATE), pinned: ['$a', '$b'], saved: ['$a', '$b'] }],
          spaces: [],
          invitations: [],
        },
      },
    });
    state = apply(state, {
      type: 'PinnedMessages',
      data: {
        account: ADA,
        room_id: PRIVATE,
        messages: [preview('$a', GRACE), preview('$b', ADA)],
      },
    });

    return apply(state, {
      type: 'SavedMessages',
      data: {
        account: ADA,
        messages: ['$a', '$b'].map((id, i) => ({
          room_id: PRIVATE,
          saved_at_ms: null,
          preview: preview(id, i === 0 ? GRACE : ADA),
        })),
      },
    });
  }

  it('pinned and saved lists should leave out what ignored people said, keeping the IDs', () => {
    const state = pinnedAndSaved({ ...DEFAULT_SAFETY, ignored: [GRACE] });
    expect([
      pinnedPreviews(state, ADA, PRIVATE)?.map((p) => p.event_id),
      savedMessages(state, ADA)?.map((m) => m.preview.event_id),
      state.accounts[0]!.rooms[0]!.pinned,
    ]).toEqual([['$b'], ['$b'], ['$a', '$b']]);
  });

  it('More on an attachment held only in a thread should find it', () => {
    const state = apply(signedIn(), {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: { room_id: PRIVATE, thread: '$root' },
        diffs: [
          {
            type: 'Reset',
            data: [messageEntry('$shot', { content: { type: 'Attachment', data: photo } })],
          },
        ],
      },
    });

    expect(previewMessage(state, ADA, roomKey(PRIVATE), '$shot')?.event_id).toBe('$shot');
  });

  it('More on a file rebuilt from Room Info’s list should find it', () => {
    const file = {
      event_id: '$f',
      sender_id: GRACE,
      sender_name: 'Grace',
      timestamp: 1,
      attachment: photo,
    };

    const state = apply(signedIn(), {
      type: 'Files',
      data: { account: ADA, room_id: PRIVATE, files: [file] },
    });

    expect(previewMessage(state, ADA, roomKey(PRIVATE), '$f')?.event_id).toBe('$f');
  });

  it('after stopping ignoring someone, their listed file should open again', () => {
    const file = {
      event_id: '$f',
      sender_id: GRACE,
      sender_name: 'Grace',
      timestamp: 1,
      attachment: photo,
    };

    let state = signedIn();
    state = apply(state, {
      type: 'Files',
      data: { account: ADA, room_id: PRIVATE, files: [file] },
    });
    state = apply(state, {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: roomKey(PRIVATE),
        diffs: [
          {
            type: 'Reset',
            data: [messageEntry('$f', { content: { type: 'Attachment', data: photo } })],
          },
        ],
      },
    });

    const ignoring = apply(state, {
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, ignored: [GRACE] } },
    });

    const stopped = apply(ignoring, {
      type: 'Safety',
      data: { account: ADA, safety: DEFAULT_SAFETY },
    });

    expect([
      previewItems(ignoring, ADA, roomKey(PRIVATE), '$f', '$f').length,
      previewItems(stopped, ADA, roomKey(PRIVATE), '$f', '$f').map((m) => m.event_id),
    ]).toEqual([0, ['$f']]);
  });
});
