import { detachedEngine, type DetachedEngine } from '@better-matrix/matrix-native/client';
import type { Command, RoomSummary } from '@better-matrix/matrix-native/types';

import { t } from '../../../i18n';
import { ADA, GRACE, roomSummary, spaceSummary, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import {
  changeRoom,
  changeSpace,
  leaveSpace,
  loadRoomSettings,
  orderSpace,
  reachSuccessor,
  upgradeRoom,
} from '../admin';
import { moderate } from '../people';

const ROOM = '!lab:example.org';

const SPACE = '!space:example.org';

/** A store signed in to Ada, with `rooms` and one space holding three rooms. */
function setUp(rooms: RoomSummary[] = [roomSummary(ROOM)]): DetachedEngine & { store: Store } {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  detached.engine.emit({
    type: 'RoomsUpdated',
    data: {
      account: ADA,
      list: { rooms, spaces: [spaceSummary(SPACE, ['!a', '!b', '!c'])], invitations: [] },
    },
  });

  return { ...detached, store };
}

function last({ sent }: DetachedEngine): { request: number; command: Command } {
  const entry = sent[sent.length - 1];

  if (!entry) throw new Error('nothing was sent');

  return entry;
}

function finish(engine: DetachedEngine['engine'], request: number, error?: string): void {
  engine.emit({
    type: 'Finished',
    data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
  });
}

const notices = (app: { store: Store }) => app.store.getState().notices.map((n) => n.message);

const children = (app: { store: Store }) =>
  app.store.getState().accounts[0]?.spaces.find((s) => s.room_id === SPACE)?.children;

describe('room settings actions', () => {
  it('should ask for a room settings', () => {
    const app = setUp();
    loadRoomSettings(app.store, ADA, ROOM);
    expect(last(app).command).toEqual({
      type: 'LoadRoomSettings',
      data: { account: ADA, room_id: ROOM },
    });
  });

  it('should tell the user when the settings could not load', async () => {
    const app = setUp();
    loadRoomSettings(app.store, ADA, ROOM);
    finish(app.engine, last(app).request, 'forbidden');
    await Promise.resolve();
    await Promise.resolve();
    expect(notices(app)).toEqual([t('mobile.admin.notice.load', { error: 'forbidden' })]);
  });

  it('should send one setting change', () => {
    const app = setUp();
    void changeRoom(app.store, ADA, ROOM, { type: 'JoinRule', data: 'Knock' });
    expect(last(app).command).toEqual({
      type: 'ChangeRoom',
      data: { account: ADA, room_id: ROOM, setting: { type: 'JoinRule', data: 'Knock' } },
    });
  });

  it('should tell the user when a change was not saved', async () => {
    const app = setUp();
    const changed = changeRoom(app.store, ADA, ROOM, { type: 'Encrypt' });
    finish(app.engine, last(app).request, 'forbidden');
    await expect(changed).rejects.toThrow('forbidden');
    expect(notices(app)).toEqual([t('mobile.admin.notice.change', { error: 'forbidden' })]);
  });

  it('should answer an upgrade with the new room', async () => {
    const app = setUp();
    const upgraded = upgradeRoom(app.store, ADA, ROOM, '12');
    const { request, command } = last(app);
    expect(command).toEqual({
      type: 'UpgradeRoom',
      data: { account: ADA, room_id: ROOM, version: '12' },
    });
    app.engine.emit({ type: 'RoomCreated', data: { request, room_id: '!lab2:example.org' } });
    finish(app.engine, request);
    await expect(upgraded).resolves.toBe('!lab2:example.org');
  });
});

describe('space actions', () => {
  it('should send a space new order with every room', () => {
    const app = setUp();
    void orderSpace(app.store, ADA, SPACE, ['!b', '!a', '!c']);
    expect(last(app).command).toEqual({
      type: 'ChangeSpace',
      data: { account: ADA, space: SPACE, change: { type: 'Order', data: ['!b', '!a', '!c'] } },
    });
  });

  it('should show a new order at once', () => {
    const app = setUp();
    void orderSpace(app.store, ADA, SPACE, ['!c', '!a', '!b']);
    expect(children(app)).toEqual(['!c', '!a', '!b']);
  });

  it('should put the order back when the change fails', async () => {
    const app = setUp();
    const ordered = orderSpace(app.store, ADA, SPACE, ['!c', '!a', '!b']);
    finish(app.engine, last(app).request, 'forbidden');
    await expect(ordered).rejects.toThrow('forbidden');
    expect(children(app)).toEqual(['!a', '!b', '!c']);
  });

  it('should keep a later order when an earlier one fails', async () => {
    const app = setUp();
    const first = orderSpace(app.store, ADA, SPACE, ['!c', '!a', '!b']);
    const request = last(app).request;
    void orderSpace(app.store, ADA, SPACE, ['!b', '!c', '!a']);
    finish(app.engine, request, 'forbidden');
    await expect(first).rejects.toThrow('forbidden');
    expect(children(app)).toEqual(['!b', '!c', '!a']);
  });

  it('should add a room to a space', () => {
    const app = setUp();
    void changeSpace(app.store, ADA, SPACE, { type: 'Add', data: ROOM });
    expect(last(app).command).toEqual({
      type: 'ChangeSpace',
      data: { account: ADA, space: SPACE, change: { type: 'Add', data: ROOM } },
    });
  });

  it('should leave a space with the chosen rooms', () => {
    const app = setUp();
    void leaveSpace(app.store, ADA, spaceSummary(SPACE, ['!a', '!b', '!c']), new Set(['!c', '!a']));
    expect(last(app).command).toEqual({
      type: 'LeaveSpace',
      data: { account: ADA, space: SPACE, rooms: ['!a', '!c'] },
    });
  });
});

describe('moderation of people outside the room', () => {
  it.each([
    ['approve a request to join', { type: 'Admit' } as const],
    ['deny a request to join', { type: 'Deny' } as const],
    ['unban someone', { type: 'Unban' } as const],
    ['give someone a custom level', { type: 'Level', data: 20 } as const],
  ])('should %s', (_, action) => {
    const app = setUp();
    void moderate(app.store, ADA, ROOM, GRACE, action);
    expect(last(app).command).toEqual({
      type: 'ModerateMember',
      data: { account: ADA, room_id: ROOM, user_id: GRACE, action },
    });
  });
});

describe('going to an upgraded room successor', () => {
  const successor = { room_id: '!new:example.org', reason: null, via: ['example.org'] };

  it('should open a successor the user joined without sending anything', async () => {
    const app = setUp([roomSummary(ROOM), roomSummary('!new:example.org')]);
    const before = app.sent.length;
    await expect(reachSuccessor(app.store, ADA, successor)).resolves.toBe('!new:example.org');
    expect(app.sent.length).toBe(before);
  });

  it('should join a successor through its servers first', async () => {
    const app = setUp();
    const reached = reachSuccessor(app.store, ADA, successor);
    const { request, command } = last(app);
    expect(command).toEqual({
      type: 'JoinRoom',
      data: { account: ADA, room_id: '!new:example.org', via: ['example.org'] },
    });
    app.engine.emit({ type: 'RoomJoined', data: { request, room_id: '!new:example.org' } });
    finish(app.engine, request);
    await expect(reached).resolves.toBe('!new:example.org');
  });
});
