import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Draft } from '@better-matrix/matrix-native/types';

import { composer } from '../../composer';
import { KEEP_EVERY_MS } from '../../drafts';
import { Store } from '../../store';
import { roomKey } from '../../timeline';
import { setDraft } from '../composer';
import { flushDraft, followDraft } from '../drafts';
import { ADA, summary } from '../../../test/fixtures';

const KEY = roomKey('!r:x');

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  return { engine, store, sent };
}

/** Lets the store tell its listeners about the changes made so far. */
const settle = () => new Promise<void>((resolve) => queueMicrotask(resolve));

const saves = (sent: ReturnType<typeof setup>['sent']) =>
  sent.flatMap(({ command }) => (command.type === 'SaveDraft' ? [command.data.draft] : []));

// The store tells its listeners in a microtask: only timers are faked.
beforeEach(() => jest.useFakeTimers({ doNotFake: ['queueMicrotask'] }));

afterEach(() => jest.useRealTimers());

it('should ask for a composer’s kept draft the first time it shows', () => {
  const { store, sent } = setup();
  followDraft(store, ADA, KEY);
  followDraft(store, ADA, KEY);

  expect(sent.filter(({ command }) => command.type === 'LoadDraft')).toHaveLength(1);
});

it('should bring the kept draft back into the composer', () => {
  const { engine, store } = setup();
  followDraft(store, ADA, KEY);
  engine.emit({
    type: 'DraftLoaded',
    data: { account: ADA, key: KEY, draft: { body: 'hi', html: null } },
  });

  expect(composer(store.getState(), ADA, KEY).draft).toBe('hi');
});

it('should write a draft being typed once it has waited', async () => {
  const { store, sent } = setup();
  followDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'half a thought');
  await settle();
  expect(saves(sent)).toEqual([]);

  jest.advanceTimersByTime(KEEP_EVERY_MS);

  expect(saves(sent)).toEqual([{ body: 'half a thought', html: null }]);
});

it('should forget the kept draft at once when the composer empties', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  const kept: Draft = { body: 'hi', html: null };
  engine.emit({ type: 'DraftLoaded', data: { account: ADA, key: KEY, draft: kept } });
  await settle();
  setDraft(store, ADA, KEY, '');
  await settle();

  expect(saves(sent)).toEqual([null]);
});

it('should not write a kept draft back as it comes in', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  engine.emit({
    type: 'DraftLoaded',
    data: { account: ADA, key: KEY, draft: { body: 'hi', html: null } },
  });
  await settle();
  jest.advanceTimersByTime(KEEP_EVERY_MS);

  expect(saves(sent)).toEqual([]);
});

it('should write what the user left over a kept draft that came too late', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'new words');
  setDraft(store, ADA, KEY, '');
  await settle();
  engine.emit({
    type: 'DraftLoaded',
    data: { account: ADA, key: KEY, draft: { body: 'from last time', html: null } },
  });

  expect(composer(store.getState(), ADA, KEY).draft).toBe('');
  expect(saves(sent)).toEqual([null]);
});

it('should write a changed draft at once when its conversation is left', async () => {
  const { store, sent } = setup();
  followDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'leaving now');
  await settle();
  flushDraft(store, ADA, KEY);

  expect(saves(sent)).toEqual([{ body: 'leaving now', html: null }]);
});

it('should forget an account’s drafts when it signs out', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'half a thought');
  await settle();
  engine.emit({ type: 'AccountRemoved', data: ADA });
  jest.advanceTimersByTime(KEEP_EVERY_MS);

  expect(store.getState().composers).toEqual({});
  expect(saves(sent)).toEqual([]);

  engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  followDraft(store, ADA, KEY);

  expect(sent.filter(({ command }) => command.type === 'LoadDraft')).toHaveLength(2);
});
