import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { MessageHit } from '@better-matrix/matrix-native/types';

import { ADA, GRACE } from '../../../test/fixtures';
import { SEARCH_PAUSE_MS } from '../../people';
import { Store } from '../../store';
import { MessageSearcher, type MessageSearch } from '../search';

const hit: MessageHit = {
  room_id: '!room:example.org',
  event_id: '$message',
  thread: null,
  sender: GRACE,
  timestamp_ms: 100,
  body: 'hello device',
  highlights: [6, 7, 8, 9, 10, 11],
};

function setup() {
  const app = detachedEngine();
  const seen: MessageSearch[] = [];
  const search = new MessageSearcher(new Store(app.engine), ADA, (s) => seen.push(s));

  return { ...app, search, seen };
}

beforeEach(() => jest.useFakeTimers());

afterEach(() => jest.useRealTimers());

it('should search local messages once after typing rests', () => {
  const app = setup();
  app.search.type('dev');
  app.search.type('device');
  jest.advanceTimersByTime(SEARCH_PAUSE_MS);
  expect(app.sent.map((s) => s.command)).toEqual([
    { type: 'SearchMessages', data: { account: ADA, term: 'device' } },
  ]);
});

it('should clear hits immediately and cancel an obsolete request', () => {
  const app = setup();
  app.search.type('device');
  jest.advanceTimersByTime(SEARCH_PAUSE_MS);
  const request = app.sent[0]!.request;
  app.engine.emit({ type: 'MessagesFound', data: { request, messages: [hit] } });
  app.search.type('other');
  expect(app.seen[app.seen.length - 1]).toEqual({ term: 'other', messages: [], status: 'waiting' });
  expect(app.sent[1]!.command).toEqual({ type: 'Cancel', data: request });
});

it('should ignore an answer after the query changes', () => {
  const app = setup();
  app.search.type('device');
  jest.advanceTimersByTime(SEARCH_PAUSE_MS);
  const request = app.sent[0]!.request;
  app.search.type('other');
  app.engine.emit({ type: 'MessagesFound', data: { request, messages: [hit] } });
  expect(app.seen[app.seen.length - 1]!.messages).toEqual([]);
});

it('should ignore an answer after the screen closes', () => {
  const app = setup();
  app.search.type('device');
  jest.advanceTimersByTime(SEARCH_PAUSE_MS);
  app.search.close();
  const count = app.seen.length;
  app.engine.emit({
    type: 'MessagesFound',
    data: { request: app.sent[0]!.request, messages: [hit] },
  });
  expect(app.seen).toHaveLength(count);
});

it('should report a failed search so new typing can retry', async () => {
  const app = setup();
  app.search.type('device');
  jest.advanceTimersByTime(SEARCH_PAUSE_MS);
  app.engine.emit({
    type: 'Finished',
    data: { request: app.sent[0]!.request, result: { Err: 'store unavailable' } },
  });
  await Promise.resolve();
  expect(app.seen[app.seen.length - 1]!.status).toBe('failed');
});
