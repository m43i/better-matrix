import { detachedEngine, type DetachedEngine } from '@better-matrix/matrix-native/client';
import type { Command, RoomPreview } from '@better-matrix/matrix-native/types';

import { t } from '../../../i18n';
import { ADA, GRACE, person, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import { findPerson, joinRoom, previewRoom } from '../links';

function setUp(): DetachedEngine & { store: Store } {
  const detached = detachedEngine();
  const store = new Store(detached.engine);
  detached.engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  return { ...detached, store };
}

function last({ sent }: DetachedEngine): { request: number; command: Command } {
  const entry = sent[sent.length - 1];
  if (!entry) throw new Error('nothing was sent');
  return entry;
}

function finish(engine: DetachedEngine['engine'], request: number, error?: string): void {
  engine.emit({ type: 'Finished', data: { request, result: error === undefined ? { Ok: null } : { Err: error } } });
}

const rust: RoomPreview = {
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
};

describe('link actions', () => {
  it('should look a room up with the servers the link names', () => {
    const app = setUp();
    void previewRoom(app.store, ADA, '#rust:matrix.org', ['matrix.org']).catch(() => {});
    expect(last(app).command).toEqual({ type: 'PreviewRoom', data: { account: ADA, room: '#rust:matrix.org', via: ['matrix.org'] } });
  });

  it('should answer with the room the engine previewed', async () => {
    const app = setUp();
    const previewed = previewRoom(app.store, ADA, '#rust:matrix.org', []);
    const { request } = last(app);
    app.engine.emit({ type: 'RoomPreview', data: { request, preview: rust } });
    finish(app.engine, request);
    await expect(previewed).resolves.toEqual(rust);
  });

  it('should answer a join with the joined room’s ID', async () => {
    const app = setUp();
    const joined = joinRoom(app.store, ADA, '#rust:matrix.org', ['matrix.org']);
    const { request } = last(app);
    app.engine.emit({ type: 'RoomJoined', data: { request, room_id: '!rust:matrix.org' } });
    finish(app.engine, request);
    await expect(joined).resolves.toBe('!rust:matrix.org');
  });

  it('should tell the user in the desktop’s words when a join fails', async () => {
    const app = setUp();
    const joined = joinRoom(app.store, ADA, '!closed:example.org', []);
    finish(app.engine, last(app).request, 'forbidden');
    await expect(joined).rejects.toThrow('forbidden');
    expect(app.store.getState().notices.map((n) => n.message)).toEqual([t('notice.join', { error: 'forbidden' })]);
  });

  it('should find the person a link names among people found for their ID', async () => {
    const app = setUp();
    const found = findPerson(app.store, ADA, GRACE);
    const { request, command } = last(app);
    app.engine.emit({ type: 'PeopleFound', data: { request, people: [person('@gracie:example.org'), person(GRACE, 'Grace')] } });
    finish(app.engine, request);
    await expect(found).resolves.toEqual(person(GRACE, 'Grace'));
    expect(command).toEqual({ type: 'SearchPeople', data: { account: ADA, term: GRACE } });
  });
});
