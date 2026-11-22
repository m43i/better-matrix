import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Invitation } from '@better-matrix/matrix-native/types';

import { ADA, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import { afterAnswer, answerInvitation } from '../invitations';

const DESIGN: Invitation = {
  room_id: '!design:x',
  name: 'Design',
  topic: null,
  avatar: null,
  inviter: null,
  space: false,
  kind: 'Group',
  stranger: false,
};

function setup() {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  detached.engine.emit({
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms: [], spaces: [], invitations: [DESIGN] } },
  });

  return { ...detached, store };
}

const finish = (
  engine: ReturnType<typeof setup>['engine'],
  request: number,
  result: { Ok: null } | { Err: string },
) => engine.emit({ type: 'Finished', data: { request, result } });

describe('answerInvitation', () => {
  it('should join an accepted invitation and say which room it joined', async () => {
    const { store, engine, sent } = setup();
    const answer = answerInvitation(store, ADA, '!design:x', true);
    const { request, command } = sent.at(-1)!;
    expect(command).toEqual({
      type: 'JoinRoom',
      data: { account: ADA, room_id: '!design:x', via: [] },
    });
    engine.emit({ type: 'RoomJoined', data: { request, room_id: '!design:x' } });
    finish(engine, request, { Ok: null });
    await expect(answer).resolves.toEqual({ kind: 'joined', roomId: '!design:x' });
  });

  it('should leave a declined invitation without forgetting the room', async () => {
    const { store, engine, sent } = setup();
    const answer = answerInvitation(store, ADA, '!design:x', false);
    const { request, command } = sent.at(-1)!;
    expect(command).toEqual({
      type: 'LeaveRoom',
      data: { account: ADA, room_id: '!design:x', forget: false },
    });
    finish(engine, request, { Ok: null });
    await expect(answer).resolves.toEqual({ kind: 'declined' });
  });

  it('should drop an answered invitation from the list', async () => {
    const { store, engine, sent } = setup();
    const answer = answerInvitation(store, ADA, '!design:x', false);
    finish(engine, sent.at(-1)!.request, { Ok: null });
    await answer;
    expect(store.getState().accounts[0]!.invitations).toEqual([]);
  });

  it('should keep an invitation whose answer failed, to try again', async () => {
    const { store, engine, sent } = setup();
    const answer = answerInvitation(store, ADA, '!design:x', true);
    finish(engine, sent.at(-1)!.request, { Err: 'forbidden' });
    await expect(answer).resolves.toEqual({ kind: 'failed', error: 'forbidden' });
    expect(store.getState().accounts[0]!.invitations).toEqual([DESIGN]);
  });
});

describe('afterAnswer', () => {
  const joined = { kind: 'joined', roomId: '!r' } as const;

  it('a join that ends after the sheet was dismissed should not navigate', () => {
    expect(afterAnswer(joined, false, false)).toEqual({ type: 'stay' });
  });

  it('a join should open the room joined', () => {
    expect(afterAnswer(joined, false, true)).toEqual({ type: 'room', roomId: '!r' });
  });

  it('a space joined should become the scope', () => {
    expect(afterAnswer(joined, true, true)).toEqual({ type: 'space', spaceId: '!r' });
  });

  it('a failure should stay to be tried again', () => {
    expect(afterAnswer({ kind: 'failed', error: 'no' }, false, true)).toEqual({ type: 'stay' });
  });

  it('a decline should close the sheet', () => {
    expect(afterAnswer({ kind: 'declined' }, false, true)).toEqual({ type: 'close' });
  });
});
