import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Draft } from '@better-matrix/matrix-native/types';

import { composer } from '../../composer';
import { KEEP_EVERY_MS } from '../../drafts';
import { Store } from '../../store';
import { roomKey } from '../../timeline';
import { cancelEdit, sendComposer, setDraft } from '../composer';
import { startEdit } from '../../composer';
import { flushDraft, followDraft } from '../drafts';
import { ADA, message, summary } from '../../../test/fixtures';

const KEY = roomKey('!r:x');

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  return { engine, store, sent };
}

/** Lets the store tell its listeners about the changes made so far. */
const settle = () => new Promise<void>((resolve) => queueMicrotask(resolve));

/** Answers the composer's read of its kept draft (the latest one asked). */
function answer(
  { engine, sent }: Pick<ReturnType<typeof setup>, 'engine' | 'sent'>,
  draft: Draft | null,
) {
  const asked = sent.filter(({ command }) => command.type === 'LoadDraft').pop();

  if (!asked) throw new Error('no draft read asked');
  engine.emit({
    type: 'DraftLoaded',
    data: { request: asked.request, account: ADA, key: KEY, draft },
  });
}

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
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  answer({ engine, sent }, { body: 'hi', html: null });

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
  answer({ engine, sent }, kept);
  await settle();
  setDraft(store, ADA, KEY, '');
  await settle();

  expect(saves(sent)).toEqual([null]);
});

it('should not write a kept draft back as it comes in', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  answer({ engine, sent }, { body: 'hi', html: null });
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
  answer({ engine, sent }, { body: 'from last time', html: null });

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

it('should keep a late draft under an edit started meanwhile, and not erase it', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  startEdit(store, ADA, KEY, message('$mine'), 'old text');
  setDraft(store, ADA, KEY, 'old text!');
  await settle();
  answer({ engine, sent }, { body: 'from last time', html: null });
  await settle();
  jest.advanceTimersByTime(KEEP_EVERY_MS);

  expect(saves(sent)).toEqual([]);
  cancelEdit(store, ADA, KEY);
  expect(composer(store.getState(), ADA, KEY).draft).toBe('from last time');
});

it('should not bring a late draft back after a message was sent', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'sent already');
  sendComposer(store, ADA, KEY);
  await settle();
  answer({ engine, sent }, { body: 'from last time', html: null });

  expect(composer(store.getState(), ADA, KEY).draft).toBe('');
  expect(saves(sent)).toContainEqual(null);
});

it('should ignore a draft read answered after its account signed out', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  engine.emit({ type: 'AccountRemoved', data: ADA });
  answer({ engine, sent }, { body: 'from the old session', html: null });

  expect(store.getState().composers).toEqual({});
});

it('should leave a read answered after a write unheeded', async () => {
  const { engine, store, sent } = setup();
  followDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'replacement');
  await settle();
  flushDraft(store, ADA, KEY);
  setDraft(store, ADA, KEY, 'original');
  await settle();
  answer({ engine, sent }, { body: 'original', html: null });
  flushDraft(store, ADA, KEY);

  expect(saves(sent)).toEqual([
    { body: 'replacement', html: null },
    { body: 'original', html: null },
  ]);
});
