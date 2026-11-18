import vectors from '../../../../../../packages/matrix-core/src/message_links.json';
import { detachedEngine } from '@better-matrix/matrix-native/client';
import { Store } from '../../store';
import { threadKey } from '../../timeline';
import {
  forwardMessage,
  listPins,
  listSaved,
  messageLink,
  setPinned,
  setSaved,
  translationUrl,
} from '../message-actions';
import { ADA, summary } from '../../../test/fixtures';
import { parsePermalink } from '../../permalinks';

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  return { engine, store, sent };
}

it('should forward from the original account and thread to the chosen room', async () => {
  const { engine, store, sent } = setup();
  const key = threadKey('!source:x', '$root');
  const done = forwardMessage(store, ADA, key, '$reply', '!destination:x');
  expect(sent[0]?.command).toEqual({
    type: 'Forward',
    data: { account: ADA, key, event_id: '$reply', room_id: '!destination:x' },
  });
  engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Ok: null } } });
  await done;
});

it('should reject a refused forward so the destination picker can be retried', async () => {
  const { engine, store, sent } = setup();
  const done = forwardMessage(store, ADA, threadKey('!r:x', '$root'), '$m', '!other:x');
  engine.emit({
    type: 'Finished',
    data: { request: sent[0]!.request, result: { Err: 'forbidden' } },
  });
  await expect(done).rejects.toThrow('forbidden');
});

it('should unpin the selected message without changing other pins locally', () => {
  const { store, sent } = setup();
  setPinned(store, ADA, '!r:x', '$m', false);
  expect(sent[0]?.command).toEqual({
    type: 'SetPinned',
    data: { account: ADA, room_id: '!r:x', event_id: '$m', pinned: false },
  });
});

it('should keep previews of pins outside the loaded timeline', async () => {
  const { engine, store, sent } = setup();
  const done = listPins(store, ADA, '!r:x');

  const messages = [
    {
      event_id: '$old',
      sender_name: 'Grace',
      sender_id: '@grace:example.org',
      sender_avatar: null,
      timestamp_ms: null,
      body: 'Release checklist',
      thread: null,
    },
  ];

  engine.emit({ type: 'PinnedMessages', data: { account: ADA, room_id: '!r:x', messages } });
  engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Ok: null } } });
  await done;
  expect(store.getState().accounts[0]?.pins['!r:x']).toEqual(messages);
});

it('should copy links with routing information for room ids without a server', () => {
  expect(parsePermalink(messageLink('!v12', '$event/with?symbols', ADA))).toEqual({
    kind: 'room',
    id: '!v12',
    event: '$event/with?symbols',
    via: ['example.org'],
    action: null,
  });
});

it.each(vectors)('should match the shared link builders for $event', (vector) => {
  expect(messageLink(vector.room, vector.event, vector.account)).toBe(vector.link);
  expect(translationUrl(vector.body, vector.language)).toBe(vector.translation);
});

it('should save a message for the user alone', () => {
  const { store, sent } = setup();
  setSaved(store, ADA, '!r:x', '$m', true);
  expect(sent[0]?.command).toEqual({
    type: 'SetSaved',
    data: { account: ADA, room_id: '!r:x', event_id: '$m', saved: true },
  });
});

it('should list the saved messages into the account', async () => {
  const { engine, store, sent } = setup();
  const done = listSaved(store, ADA);

  const preview = {
    event_id: '$m',
    sender_name: 'Grace',
    sender_id: '@grace:x',
    sender_avatar: null,
    timestamp_ms: 1,
    body: 'Remember this',
    thread: null,
  };

  engine.emit({
    type: 'SavedMessages',
    data: { account: ADA, messages: [{ room_id: '!r:x', saved_at_ms: 2, preview }] },
  });
  engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Ok: null } } });
  await done;
  expect(store.getState().accounts[0]?.saved?.[0]?.preview.body).toBe('Remember this');
});

it('should ask for an account’s saved messages one list at a time, in order', async () => {
  const { engine, store, sent } = setup();
  const first = listSaved(store, ADA);
  const second = listSaved(store, ADA);
  await Promise.resolve();
  expect(sent.filter(({ command }) => command.type === 'ListSavedMessages')).toHaveLength(1);

  engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Ok: null } } });
  await first;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  const lists = sent.filter(({ command }) => command.type === 'ListSavedMessages');
  expect(lists).toHaveLength(2);

  engine.emit({ type: 'Finished', data: { request: lists[1]!.request, result: { Ok: null } } });
  await second;
});
