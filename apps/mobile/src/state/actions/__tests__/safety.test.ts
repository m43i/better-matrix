import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Invitation } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, summary } from '../../../test/fixtures';
import { DEFAULT_SAFETY, revealKey } from '../../safety';
import { Store } from '../../store';
import { changeSafety, ignoreInviter, report, revealMedia, setIgnored } from '../safety';

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
