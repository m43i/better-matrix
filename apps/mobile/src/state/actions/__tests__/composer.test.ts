import { detachedEngine } from '@better-matrix/matrix-native/client';

import { t } from '../../../i18n';
import { ADA, message, summary } from '../../../test/fixtures';
import { composer, startEdit, startReply } from '../../composer';
import { newPollForm } from '../../pollForm';
import { Store } from '../../store';
import { roomKey, threadKey } from '../../timeline';
import {
  addAttachments,
  cancelEdit,
  insertIntoDraft,
  keyOf,
  react,
  removeAttachment,
  sendComposer,
  sendPoll,
  sendTyping,
  sendVoiceMemo,
  setDraft,
} from '../composer';

const ROOM = '!lab:example.org';

const KEY = roomKey(ROOM);

function setUp() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  const commands = () => sent.map((s) => s.command);

  const fail = (index: number, error: string) =>
    engine.emit({
      type: 'Finished',
      data: { request: sent[index]!.request, result: { Err: error } },
    });

  const succeed = (index: number) =>
    engine.emit({
      type: 'Finished',
      data: { request: sent[index]!.request, result: { Ok: null } },
    });

  return { store, commands, fail, succeed };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const pdf = {
  path: '/cache/a.pdf',
  name: 'a.pdf',
  kind: 'file' as const,
  uri: 'file:///cache/a.pdf',
};

describe('composer actions', () => {
  it('should send the draft and clear it', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, 'hello');
    sendComposer(store, ADA, KEY);
    expect(commands()).toEqual([
      {
        type: 'Send',
        data: {
          account: ADA,
          key: KEY,
          reply_to: null,
          message: { type: 'Text', data: { body: 'hello', html: null } },
        },
      },
    ]);
    expect(composer(store.getState(), ADA, KEY).draft).toBe('');
  });

  it('should keep each timeline its own draft', () => {
    const { store } = setUp();
    setDraft(store, ADA, KEY, 'room');
    setDraft(store, ADA, threadKey(ROOM, '$root'), 'thread');
    expect(composer(store.getState(), ADA, KEY).draft).toBe('room');
  });

  it('should send a reply to the message replied to', () => {
    const { store, commands } = setUp();
    startReply(store, ADA, KEY, message('$q'));
    setDraft(store, ADA, KEY, 'yes');
    sendComposer(store, ADA, KEY);
    const [command] = commands();
    expect(command?.type === 'Send' && command.data.reply_to).toBe('$q');
  });

  it('should send nothing from an empty composer', () => {
    const { store, commands } = setUp();
    expect(sendComposer(store, ADA, KEY)).toBe(false);
    expect(commands()).toEqual([]);
  });

  it('should send attachments after the text as files on disk', () => {
    const { store, commands } = setUp();
    addAttachments(store, ADA, KEY, [pdf]);
    setDraft(store, ADA, KEY, 'see');
    sendComposer(store, ADA, KEY);
    const files = commands().filter((c) => c.type === 'Send' && c.data.message.type === 'File');
    expect(files.map((c) => c.type === 'Send' && c.data.message)).toEqual([
      { type: 'File', data: { type: 'Path', data: '/cache/a.pdf' } },
    ]);
  });

  it('should forget a removed attachment', () => {
    const { store } = setUp();
    addAttachments(store, ADA, KEY, [pdf]);
    removeAttachment(store, ADA, KEY, pdf.path);
    expect(composer(store.getState(), ADA, KEY).attachments).toEqual([]);
  });

  it('should save an edit as an edit of the message', () => {
    const { store, commands } = setUp();
    startEdit(store, ADA, KEY, message('$mine', 'helo'), 'helo');
    setDraft(store, ADA, KEY, 'hello');
    sendComposer(store, ADA, KEY);
    expect(commands()).toEqual([
      {
        type: 'Edit',
        data: { account: ADA, key: KEY, event_id: '$mine', body: 'hello', html: null },
      },
    ]);
  });

  it('should bring the draft back when an edit is cancelled', () => {
    const { store } = setUp();
    setDraft(store, ADA, KEY, 'half a thought');
    startEdit(store, ADA, KEY, message('$mine', 'old'), 'old');
    cancelEdit(store, ADA, KEY);
    expect(composer(store.getState(), ADA, KEY).draft).toBe('half a thought');
  });

  it('should end an edit when replying, bringing the draft back', () => {
    const { store } = setUp();
    setDraft(store, ADA, KEY, 'draft');
    startEdit(store, ADA, KEY, message('$mine', 'old'), 'old');
    startReply(store, ADA, KEY, message('$q'));
    const after = composer(store.getState(), ADA, KEY);
    expect([after.draft, after.editing, after.replyTo?.event_id]).toEqual(['draft', null, '$q']);
  });

  it('a refused edit should come back into the composer as it was sent', async () => {
    const { store, fail } = setUp();
    startEdit(store, ADA, KEY, message('$mine', 'helo'), 'helo');
    setDraft(store, ADA, KEY, 'hello');
    sendComposer(store, ADA, KEY);
    fail(0, 'offline');
    await flush();
    const after = composer(store.getState(), ADA, KEY);
    expect([after.editing?.event_id, after.draft]).toEqual(['$mine', 'hello']);
  });

  it('a refused edit should set aside what was typed since, bringing it back when dropped', async () => {
    const { store, fail } = setUp();
    startEdit(store, ADA, KEY, message('$mine', 'helo'), 'helo');
    setDraft(store, ADA, KEY, 'hello');
    sendComposer(store, ADA, KEY);
    setDraft(store, ADA, KEY, 'newer thought');
    fail(0, 'offline');
    await flush();
    cancelEdit(store, ADA, KEY);
    expect(composer(store.getState(), ADA, KEY).draft).toBe('newer thought');
  });

  it('a refused edit should wait while another edit holds the composer', async () => {
    const { store, fail } = setUp();
    startEdit(store, ADA, KEY, message('$mine', 'helo'), 'helo');
    setDraft(store, ADA, KEY, 'hello');
    sendComposer(store, ADA, KEY);
    startEdit(store, ADA, KEY, message('$other', 'one'), 'one');
    fail(0, 'offline');
    await flush();
    expect(composer(store.getState(), ADA, KEY).editing?.event_id).toBe('$other');
    cancelEdit(store, ADA, KEY);
    const after = composer(store.getState(), ADA, KEY);
    expect([after.editing?.event_id, after.draft]).toEqual(['$mine', 'hello']);
  });

  it('a refused edit should not replace a newer edit of the same message', async () => {
    const { store, fail } = setUp();

    for (const text of ['first fix', 'second fix']) {
      startEdit(store, ADA, KEY, message('$mine', 'helo'), 'helo');
      setDraft(store, ADA, KEY, text);
      sendComposer(store, ADA, KEY);
    }

    startEdit(store, ADA, KEY, message('$other', 'one'), 'one');
    fail(1, 'offline');
    fail(0, 'offline');
    await flush();
    cancelEdit(store, ADA, KEY);
    expect(composer(store.getState(), ADA, KEY).draft).toBe('second fix');
  });

  it('a refused edit should stay away once a newer edit of the message went through', async () => {
    const { store, fail, succeed } = setUp();

    for (const text of ['first fix', 'second fix']) {
      startEdit(store, ADA, KEY, message('$mine', 'helo'), 'helo');
      setDraft(store, ADA, KEY, text);
      sendComposer(store, ADA, KEY);
    }

    succeed(1);
    fail(0, 'offline');
    await flush();
    expect(composer(store.getState(), ADA, KEY).editing).toBeNull();
  });

  it('should put a picked emoji at the caret', () => {
    const { store } = setUp();
    setDraft(store, ADA, KEY, 'ab', { start: 1, end: 1 });
    insertIntoDraft(store, ADA, KEY, '🎉');
    expect(composer(store.getState(), ADA, KEY).draft).toBe('a🎉b');
  });

  it('should tell the user a message was not sent', async () => {
    const { store, fail } = setUp();
    setDraft(store, ADA, KEY, 'hello');
    sendComposer(store, ADA, KEY);
    fail(0, 'offline');
    await flush();
    expect(store.getState().notices.map((n) => n.message)).toEqual([
      t('notice.not_sent', { error: 'offline' }),
    ]);
  });

  it('should send a voice memo with its waveform', () => {
    const { store, commands } = setUp();
    sendVoiceMemo(store, ADA, KEY, '/cache/v.m4a', 3200, [12, 700]);
    const [command] = commands();
    expect(
      command?.type === 'Send' &&
        command.data.message.type === 'Voice' &&
        command.data.message.data.waveform,
    ).toEqual([12, 700]);
  });

  it('should send a voice memo with its length', () => {
    const { store, commands } = setUp();
    sendVoiceMemo(store, ADA, KEY, '/cache/v.m4a', 3200);
    const [command] = commands();
    expect(command?.type === 'Send' && command.data.message).toEqual({
      type: 'Voice',
      data: { upload: { type: 'Path', data: '/cache/v.m4a' }, duration_ms: 3200, waveform: [] },
    });
  });

  it('should send a ready poll', () => {
    const { store, commands } = setUp();
    expect(
      sendPoll(store, ADA, KEY, {
        ...newPollForm(),
        question: 'Lunch?',
        answers: ['Pizza', 'Soup'],
      }),
    ).toBe(true);
    const [command] = commands();
    expect(command?.type === 'Send' && command.data.message.type).toBe('Poll');
  });

  it('should not send a poll that is not ready', () => {
    const { store, commands } = setUp();
    expect(sendPoll(store, ADA, KEY, newPollForm())).toBe(false);
    expect(commands()).toEqual([]);
  });

  it('should react in the timeline that shows the message', () => {
    const { store, commands } = setUp();
    react(store, ADA, keyOf(ROOM, '$root'), '$m', '👍');
    expect(commands()).toEqual([
      {
        type: 'React',
        data: { account: ADA, key: threadKey(ROOM, '$root'), event_id: '$m', emoji: '👍' },
      },
    ]);
  });

  it('should remember an emoji used for a reaction', () => {
    const { store } = setUp();
    react(store, ADA, KEY, '$m', '🎉');
    expect(store.getState().recentEmoji).toEqual(['🎉']);
  });

  it('should tell the room the user is typing', () => {
    const { store, commands } = setUp();
    sendTyping(store, ADA, ROOM, true);
    expect(commands()).toEqual([
      { type: 'Typing', data: { account: ADA, room_id: ROOM, typing: true } },
    ]);
  });
});
