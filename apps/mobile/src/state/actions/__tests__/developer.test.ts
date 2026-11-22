import { detachedEngine } from '@better-matrix/matrix-native/client';

import { ADA, summary } from '../../../test/fixtures';
import { DEFAULT_SETTINGS, parseSettings } from '../../settings';
import { Store } from '../../store';
import { addWidget, listWidgets, removeWidget, viewSource } from '../developer';

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  return { engine, store, sent };
}

const source = {
  room_id: '!r:x',
  event_id: '$e',
  json: '{\n  "type": "m.room.message"\n}',
  encrypted: null,
};

it('should answer a source request with the source the engine reports', async () => {
  const { engine, store, sent } = setup();
  const answer = viewSource(store, ADA, '!r:x', '$e');
  const request = sent[0]!.request;
  expect(sent[0]?.command).toEqual({
    type: 'ViewSource',
    data: { account: ADA, room_id: '!r:x', event_id: '$e' },
  });
  engine.emit({ type: 'EventSource', data: { request, source } });
  engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
  await expect(answer).resolves.toEqual(source);
});

it('should reject a source request the engine failed', async () => {
  const { engine, store, sent } = setup();
  const answer = viewSource(store, ADA, '!r:x', '$e');
  engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'gone' } } });
  await expect(answer).rejects.toThrow('gone');
});

it('should keep a room’s listed widgets', () => {
  const { engine, store, sent } = setup();
  listWidgets(store, ADA, '!r:x');
  expect(sent[0]?.command).toEqual({
    type: 'ListWidgets',
    data: { account: ADA, room_id: '!r:x' },
  });
  const widgets = { widgets: [], can_add: true };
  engine.emit({ type: 'Widgets', data: { account: ADA, room_id: '!r:x', widgets } });
  expect(store.getState().accounts[0]?.widgets['!r:x']).toEqual(widgets);
});

it('should stop loading a room’s widgets when listing them failed', async () => {
  const { engine, store, sent } = setup();
  listWidgets(store, ADA, '!r:x');
  engine.emit({ type: 'Finished', data: { request: sent[0]!.request, result: { Err: 'nope' } } });
  await Promise.resolve();
  await Promise.resolve();
  expect(store.getState().accounts[0]?.widgets['!r:x']).toEqual({ widgets: [], can_add: false });
});

it('should add a widget with its name and address trimmed', () => {
  const { store, sent } = setup();
  void addWidget(store, ADA, '!r:x', ' Board ', ' https://board.example ');
  expect(sent[0]?.command).toEqual({
    type: 'AddWidget',
    data: { account: ADA, room_id: '!r:x', name: 'Board', url: 'https://board.example' },
  });
});

it('should remove a widget by its ID', () => {
  const { store, sent } = setup();
  removeWidget(store, ADA, '!r:x', 'w1');
  expect(sent[0]?.command).toEqual({
    type: 'RemoveWidget',
    data: { account: ADA, room_id: '!r:x', widget_id: 'w1' },
  });
});

it('should keep hidden events left out unless they were turned on', () => {
  expect(DEFAULT_SETTINGS.showHiddenEvents).toBe(false);
  expect(parseSettings(JSON.stringify({ showHiddenEvents: true })).showHiddenEvents).toBe(true);
  expect(parseSettings(JSON.stringify({ showHiddenEvents: 'yes' })).showHiddenEvents).toBe(false);
});
