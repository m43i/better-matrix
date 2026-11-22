import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Invitation } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, message, messageEntry, summary } from '../../../test/fixtures';
import { activeRoomList, shownTimeline } from '../../selectors';
import { roomKey } from '../../timeline';
import { DEFAULT_SAFETY, revealKey } from '../../safety';
import { Store } from '../../store';
import {
  afterReport,
  changeSafety,
  ignoreInviter,
  report,
  revealMedia,
  setIgnored,
} from '../safety';

const SPAM: Invitation = {
  room_id: '!spam:x',
  name: 'Free crypto',
  topic: null,
  avatar: null,
  inviter: { id: '@prizes:spam.example', name: 'Prize Desk', avatar: null },
  space: false,
  kind: 'Group',
  stranger: true,
};

function setup() {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  detached.engine.emit({
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms: [], spaces: [], invitations: [SPAM] } },
  });

  const finish = (request: number, result: { Ok: null } | { Err: string } = { Ok: null }) =>
    detached.engine.emit({ type: 'Finished', data: { request, result } });

  return { ...detached, store, finish };
}

const safetyOf = (store: Store) => store.getState().accounts[0]!.safety;

describe('changeSafety', () => {
  it('should ask the engine to save the change', () => {
    const { store, sent } = setup();
    void changeSafety(store, ADA, { type: 'Media', data: 'Never' });
    expect(sent.at(-1)!.command).toEqual({
      type: 'SetSafety',
      data: { account: ADA, change: { type: 'Media', data: 'Never' } },
    });
  });

  it('should show the change before the engine answers', () => {
    const { store } = setup();
    void changeSafety(store, ADA, { type: 'HideStrangerInvites', data: true });
    expect(safetyOf(store).hide_stranger_invites).toBe(true);
  });

  it('a change that fails to save should be put back and say why', async () => {
    const { store, sent, finish } = setup();
    const change = changeSafety(store, ADA, { type: 'InvitePictures', data: false });
    finish(sent.at(-1)!.request, { Err: 'offline' });
    await expect(change).rejects.toThrow();
    expect(safetyOf(store)).toEqual(DEFAULT_SAFETY);
    expect(store.getState().notices.at(-1)!.message).toContain('offline');
  });
});

describe('report', () => {
  it('should send the trimmed reason and say it went', async () => {
    const { store, sent, finish } = setup();
    const target = { type: 'Room' as const, data: { room_id: '!spam:x' } };
    const done = report(store, ADA, target, '  scam  ');
    expect(sent.at(-1)!.command).toEqual({
      type: 'Report',
      data: { account: ADA, target, reason: 'scam' },
    });
    finish(sent.at(-1)!.request);
    await done;
    expect(store.getState().notices.at(-1)!.message).toBe('Report sent.');
  });

  it('should also ignore the person when asked to', async () => {
    const { store, sent, finish } = setup();
    const target = { type: 'Person' as const, data: { user_id: GRACE } };
    const done = report(store, ADA, target, '', GRACE);
    finish(sent.at(-1)!.request);
    await Promise.resolve();
    await Promise.resolve();
    const ignore = sent.at(-1)!;
    expect(ignore.command).toEqual({
      type: 'SetSafety',
      data: { account: ADA, change: { type: 'Ignore', data: { user_id: GRACE, ignored: true } } },
    });
    finish(ignore.request);
    await done;
  });

  it('a report that fails should reject, so the sheet stays', async () => {
    const { store, sent, finish } = setup();
    const done = report(store, ADA, { type: 'Person', data: { user_id: GRACE } }, '');
    finish(sent.at(-1)!.request, { Err: 'unsupported' });
    await expect(done).rejects.toThrow();
  });
});

describe('ignoreInviter', () => {
  it('should ignore the inviter, then decline', async () => {
    const { store, sent, finish } = setup();
    const answer = ignoreInviter(store, ADA, SPAM);
    finish(sent.at(-1)!.request);
    await Promise.resolve();
    await Promise.resolve();
    expect(sent.map((s) => s.command.type)).toEqual(['SetSafety', 'LeaveRoom']);
    finish(sent.at(-1)!.request);
    await expect(answer).resolves.toEqual({ kind: 'declined' });
  });

  it('should keep the invitation when ignoring fails', async () => {
    const { store, sent, finish } = setup();
    const answer = ignoreInviter(store, ADA, SPAM);
    finish(sent.at(-1)!.request, { Err: 'offline' });
    await expect(answer).resolves.toEqual({ kind: 'failed', error: null });
    expect(sent.map((s) => s.command.type)).toEqual(['SetSafety']);
  });
});

describe('setIgnored', () => {
  it('stopping ignoring someone should take them off the list at once', () => {
    const { store, engine } = setup();
    engine.emit({
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, ignored: [GRACE] } },
    });
    void setIgnored(store, ADA, GRACE, false);
    expect(safetyOf(store).ignored).toEqual([]);
  });
});

describe('revealMedia', () => {
  it('should remember the message whose media the user showed', () => {
    const { store } = setup();
    revealMedia(store, ADA, '$photo');
    expect(store.getState().revealedMedia).toEqual({ [revealKey(ADA, '$photo')]: true });
  });
});

describe('afterReport', () => {
  it('a report sent should close the sheet', () => expect(afterReport(true, true)).toBe('close'));

  it('a report sent after the user left the sheet should move nothing', () =>
    expect(afterReport(true, false)).toBe('stay'));

  it('a report that failed should stay to try again', () =>
    expect(afterReport(false, true)).toBe('stay'));
});

describe('ignoring someone, whichever answer comes first', () => {
  /** Ada with a room whose newest message is Grace's. */
  function withGracesMessage() {
    const setup_ = setup();
    setup_.engine.emit({
      type: 'LatestMessage',
      data: { account: ADA, room_id: '!room:x', message: message('$hi') },
    });

    return setup_;
  }

  const latest = (store: Store) => store.getState().accounts[0]!.latest['!room:x'];

  it('an ignore shown before the engine confirms it should drop their preview, and keep it gone', async () => {
    const { store, engine, sent, finish } = withGracesMessage();
    const ignored = setIgnored(store, ADA, GRACE, true);
    engine.emit({
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, ignored: [GRACE] } },
    });
    finish(sent.at(-1)!.request);
    await ignored;
    expect(latest(store)).toBeUndefined();
  });

  it('a message from someone ignored that syncs afterwards should not show in the list', () => {
    const { store, engine } = withGracesMessage();
    engine.emit({
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, ignored: [GRACE] } },
    });
    engine.emit({
      type: 'LatestMessage',
      data: { account: ADA, room_id: '!room:x', message: message('$again') },
    });
    expect(latest(store)).toBeUndefined();
  });

  it('an ignore still on its way should hide their preview from the list without removing it', () => {
    const { store } = withGracesMessage();
    void setIgnored(store, ADA, GRACE, true);
    const state = store.getState();
    expect([activeRoomList(state)!.latest['!room:x'], latest(store)?.event_id]).toEqual([
      undefined,
      '$hi',
    ]);
  });

  it('an ignore that fails should give their preview and messages back', async () => {
    const { store, engine, sent, finish } = withGracesMessage();
    engine.emit({
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: roomKey('!room:x'),
        diffs: [{ type: 'Reset', data: [messageEntry('$hi')] }],
      },
    });
    const ignored = setIgnored(store, ADA, GRACE, true);
    finish(sent.at(-1)!.request, { Err: 'offline' });
    await expect(ignored).rejects.toThrow();
    const state = store.getState();
    expect([
      activeRoomList(state)!.latest['!room:x']?.event_id,
      shownTimeline(state, ADA, roomKey('!room:x')).entries[0]?.kind.type,
    ]).toEqual(['$hi', 'Message']);
  });

  it('an ignore still on its way should draw their messages as nothing', () => {
    const { store, engine } = withGracesMessage();
    engine.emit({
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: roomKey('!room:x'),
        diffs: [{ type: 'Reset', data: [messageEntry('$hi')] }],
      },
    });
    void setIgnored(store, ADA, GRACE, true);
    expect(shownTimeline(store.getState(), ADA, roomKey('!room:x')).entries[0]?.kind.type).toBe(
      'Other',
    );
  });
});

describe('changes that fail in sequence', () => {
  it('should fall back to what the engine reported, never to an earlier change still pending', async () => {
    const { store, sent, finish } = setup();
    const media = changeSafety(store, ADA, { type: 'Media', data: 'Never' });
    const mediaRequest = sent.at(-1)!.request;
    const pictures = changeSafety(store, ADA, { type: 'InvitePictures', data: false });
    const picturesRequest = sent.at(-1)!.request;

    finish(mediaRequest, { Err: 'offline' });
    await expect(media).rejects.toThrow();
    expect(safetyOf(store)).toEqual({ ...DEFAULT_SAFETY, invite_pictures: false });

    finish(picturesRequest, { Err: 'offline' });
    await expect(pictures).rejects.toThrow();
    expect(safetyOf(store)).toEqual(DEFAULT_SAFETY);
  });

  it('a change confirmed while another fails should stay', async () => {
    const { store, engine, sent, finish } = setup();
    const media = changeSafety(store, ADA, { type: 'Media', data: 'Never' });
    const mediaRequest = sent.at(-1)!.request;
    const pictures = changeSafety(store, ADA, { type: 'InvitePictures', data: false });
    engine.emit({
      type: 'Safety',
      data: { account: ADA, safety: { ...DEFAULT_SAFETY, invite_pictures: false } },
    });
    finish(sent.at(-1)!.request);
    await pictures;

    finish(mediaRequest, { Err: 'offline' });
    await expect(media).rejects.toThrow();
    expect(safetyOf(store)).toEqual({ ...DEFAULT_SAFETY, invite_pictures: false });
  });
});
