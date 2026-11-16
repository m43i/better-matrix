import { detachedEngine, type DetachedEngine } from '@better-matrix/matrix-native/client';
import type { Command, Members } from '@better-matrix/matrix-native/types';

import { t } from '../../../i18n';
import { ADA, GRACE, roomSummary, summary } from '../../../test/fixtures';
import { EMPTY_SEARCH, type PeopleSearch, SEARCH_PAUSE_MS } from '../../people';
import { Store } from '../../store';
import {
  PeopleSearcher,
  create,
  createSpace,
  fileForSharing,
  invite,
  listAllMembers,
  listThreads,
  openDirectChat,
  setNotificationMode,
} from '../people';

const ROOM = '!lab:example.org';

/** A store signed in to Ada, with one room. */
function setUp(): DetachedEngine & { store: Store } {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  detached.engine.emit({
    type: 'RoomsUpdated',
    data: { account: ADA, list: { rooms: [roomSummary(ROOM)], spaces: [], invitations: [] } },
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

const members: Members = {
  people: [],
  complete: true,
  can_invite: true,
  own_level: 100,
  can_kick: true,
  can_ban: true,
  can_set_roles: true,
};

describe('people actions', () => {
  it('should fetch everyone in a room when the list is not complete', () => {
    const app = setUp();
    listAllMembers(app.store, ADA, ROOM);
    expect(last(app).command).toEqual({
      type: 'ListMembers',
      data: { account: ADA, room_id: ROOM, all: true },
    });
  });

  it('should not fetch the members again once the list is complete', () => {
    const app = setUp();
    app.engine.emit({
      type: 'Members',
      data: { request: null, account: ADA, room_id: ROOM, members },
    });
    const before = app.sent.length;
    listAllMembers(app.store, ADA, ROOM);
    expect(app.sent.length).toBe(before);
  });

  it('should end the threads loading with an empty list when listing them fails', async () => {
    const app = setUp();
    listThreads(app.store, ADA, ROOM);
    finish(app.engine, last(app).request, 'offline');
    await new Promise(process.nextTick);
    expect(app.store.getState().accounts[0]?.threads[ROOM]).toEqual([]);
  });

  it('should show a room new notification mode at once', () => {
    const app = setUp();
    setNotificationMode(app.store, ADA, ROOM, 'Mute');
    expect(app.store.getState().accounts[0]?.rooms[0]).toMatchObject({
      notifications: 'Mute',
      custom_notifications: true,
    });
  });

  it('should send a room new notification mode', () => {
    const app = setUp();
    setNotificationMode(app.store, ADA, ROOM, null);
    expect(last(app).command).toEqual({
      type: 'SetNotificationMode',
      data: { account: ADA, room_id: ROOM, mode: null },
    });
  });

  it('should answer a direct chat with the room the engine opened', async () => {
    const app = setUp();
    const chat = openDirectChat(app.store, ADA, GRACE);
    const { request } = last(app);
    app.engine.emit({ type: 'DirectChat', data: { request, room_id: '!dm:example.org' } });
    finish(app.engine, request);
    await expect(chat).resolves.toBe('!dm:example.org');
  });

  it('should tell the user in the desktop words when an invite fails', async () => {
    const app = setUp();
    const invited = invite(app.store, ADA, ROOM, GRACE);
    finish(app.engine, last(app).request, 'forbidden');
    await expect(invited).rejects.toThrow('forbidden');
    expect(app.store.getState().notices.map((n) => n.message)).toEqual([
      t('notice.invite', { error: 'forbidden' }),
    ]);
  });

  it('should answer a new room with its ID', async () => {
    const app = setUp();

    const made = create(app.store, {
      type: 'CreateRoom',
      data: { account: ADA, name: 'Lab', topic: null, space: null },
    });

    const { request } = last(app);
    app.engine.emit({ type: 'RoomCreated', data: { request, room_id: '!new:example.org' } });
    finish(app.engine, request);
    await expect(made).resolves.toBe('!new:example.org');
  });

  it('should show a new space in the account room list', async () => {
    const app = setUp();

    const made = createSpace(app.store, {
      type: 'CreateSpace',
      data: { account: ADA, name: 'Club' },
    });

    const { request } = last(app);
    app.engine.emit({ type: 'RoomCreated', data: { request, room_id: '!club:example.org' } });
    finish(app.engine, request);
    await made;
    expect(app.store.getState().accounts[0]?.scope).toEqual({ space: '!club:example.org' });
  });

  it('should fetch a file in full before sharing it', async () => {
    const app = setUp();

    const attachment = {
      kind: 'File' as const,
      name: 'notes.pdf',
      size: null,
      media: 'mxc-notes',
      thumbnail: null,
      dimensions: null,
    };

    const uri = fileForSharing(app.store, ADA, attachment);
    const { request, command } = last(app);
    app.engine.emit({
      type: 'MediaFile',
      data: { request, media: 'mxc-notes', size: { type: 'Original' }, path: '/cache/notes.pdf' },
    });
    finish(app.engine, request);
    expect(command).toEqual({
      type: 'FetchMedia',
      data: { account: ADA, media: 'mxc-notes', size: { type: 'Original' } },
    });
    await expect(uri).resolves.toBe('file:///cache/notes.pdf');
  });
});

describe('PeopleSearcher', () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  function searcher(app: ReturnType<typeof setUp>) {
    const seen: PeopleSearch[] = [EMPTY_SEARCH];

    return { searcher: new PeopleSearcher(app.store, ADA, (s) => seen.push(s)), seen };
  }

  it('should ask whom to suggest', () => {
    const app = setUp();
    searcher(app).searcher.suggest();
    expect(last(app).command).toEqual({ type: 'SuggestPeople', data: ADA });
  });

  it('should search once the text rests', () => {
    const app = setUp();
    const { searcher: s } = searcher(app);
    const before = app.sent.length;
    s.type('a');
    s.type('ad');
    s.type('ada');
    jest.advanceTimersByTime(SEARCH_PAUSE_MS);
    expect(app.sent.slice(before).map((e) => e.command)).toEqual([
      { type: 'SearchPeople', data: { account: ADA, term: 'ada' } },
    ]);
  });

  it('should not search while the text is still being typed', () => {
    const app = setUp();
    const { searcher: s } = searcher(app);
    const before = app.sent.length;
    s.type('ada');
    jest.advanceTimersByTime(SEARCH_PAUSE_MS - 1);
    expect(app.sent.length).toBe(before);
  });

  it('should list who the latest search found', () => {
    const app = setUp();
    const { searcher: s } = searcher(app);
    s.type('grace');
    jest.advanceTimersByTime(SEARCH_PAUSE_MS);
    app.engine.emit({
      type: 'PeopleFound',
      data: { request: last(app).request, people: [{ id: GRACE, name: 'Grace', avatar: null }] },
    });
    expect(s.current.people.map((p) => p.id)).toEqual([GRACE]);
  });

  it('should drop what an older search found', () => {
    const app = setUp();
    const { searcher: s } = searcher(app);
    s.type('ada');
    jest.advanceTimersByTime(SEARCH_PAUSE_MS);
    const old = last(app).request;
    s.type('grace');
    jest.advanceTimersByTime(SEARCH_PAUSE_MS);
    app.engine.emit({
      type: 'PeopleFound',
      data: { request: old, people: [{ id: ADA, name: 'Ada', avatar: null }] },
    });
    expect(s.current.people).toEqual([]);
  });

  it('should hear nothing once closed', () => {
    const app = setUp();
    const { searcher: s, seen } = searcher(app);
    s.suggest();
    const count = seen.length;
    s.close();
    app.engine.emit({
      type: 'PeopleFound',
      data: { request: last(app).request, people: [{ id: GRACE, name: 'Grace', avatar: null }] },
    });
    expect(seen.length).toBe(count);
  });
});
