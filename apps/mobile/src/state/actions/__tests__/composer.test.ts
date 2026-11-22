import { detachedEngine } from '@better-matrix/matrix-native/client';

import { t } from '../../../i18n';
import { ADA, message, summary, withContent } from '../../../test/fixtures';
import { composer, startEdit, startReply } from '../../composer';
import { applyFormat, continueBlock } from '../../format';
import { editDraftOf } from '../../messages';
import { newPollForm } from '../../pollForm';
import { Store } from '../../store';
import { roomKey, threadKey } from '../../timeline';
import { emotePill, pillAt, userPill } from '../../pills';
import {
  addAttachments,
  cancelEdit,
  completeDraft,
  insertIntoDraft,
  keyOf,
  react,
  removeAttachment,
  scheduleComposer,
  sendComposer,
  sendPoll,
  sendTyping,
  sendVoiceMemo,
  setDraft,
  spliceDraft,
} from '../composer';

/** `m.mentions` for a message that mentions nobody. */
const NOBODY = { user_ids: [], room: false };

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
          message: { type: 'Text', data: { body: 'hello', html: null, mentions: NOBODY } },
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
        data: {
          account: ADA,
          key: KEY,
          event_id: '$mine',
          body: 'hello',
          html: null,
          mentions: NOBODY,
        },
      },
    ]);
  });

  it('should keep a spoiler over several lines hidden through an edit of one of its words', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, '/spoiler he\ndies\n\nsadly');
    sendComposer(store, ADA, KEY);
    const sent = commands()[0];

    if (sent?.type !== 'Send' || sent.data.message.type !== 'Text')
      throw new Error(`not a text message: ${JSON.stringify(sent)}`);
    const mine = withContent('$mine', { type: 'Text', data: sent.data.message.data });
    const { draft, pills } = editDraftOf(mine);
    startEdit(store, ADA, KEY, mine, draft, pills);
    setDraft(store, ADA, KEY, draft.replace('dies', 'lives'));
    sendComposer(store, ADA, KEY);
    const edit = commands()[1];

    expect(edit).toMatchObject({
      type: 'Edit',
      data: {
        body: '[Spoiler]',
        html:
          '<p><span data-mx-spoiler>he</span></p><p><span data-mx-spoiler>lives</span></p>' +
          '<p></p><p><span data-mx-spoiler>sadly</span></p>',
      },
    });
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

  it('a refused edit should come back with its pills and the message’s own', async () => {
    const { store, fail } = setUp();
    const grace = pillAt(userPill('@grace:x', 'Grace'), 3);
    startEdit(store, ADA, KEY, message('$mine', 'hi Grace'), 'hi @Grace', [grace]);
    setDraft(store, ADA, KEY, 'hi @Grace!');
    sendComposer(store, ADA, KEY);
    fail(0, 'offline');
    await flush();
    const after = composer(store.getState(), ADA, KEY);
    expect([after.draft, after.pills, after.setAside?.originalPills]).toEqual([
      'hi @Grace!',
      [grace],
      [grace],
    ]);
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

describe('pills in the composer', () => {
  const grace = userPill('@grace:x', 'Grace');

  const otherGrace = userPill('@grace:y', 'Grace');

  it('should keep the pill a picked suggestion inserted', () => {
    const { store } = setUp();
    setDraft(store, ADA, KEY, 'hi @Gr');
    completeDraft(store, ADA, KEY, 'hi @Grace ', { start: 10, end: 10 }, pillAt(grace, 3));
    expect(composer(store.getState(), ADA, KEY).pills).toEqual([pillAt(grace, 3)]);
  });

  it('should bind a picked pill to the words it replaced, not the same words before them', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, '@Grace said @Gr');
    completeDraft(store, ADA, KEY, '@Grace said @Grace', { start: 18, end: 18 }, pillAt(grace, 12));
    sendComposer(store, ADA, KEY);
    expect(commands()[0]).toMatchObject({
      data: {
        message: {
          data: {
            body: '@Grace said Grace',
            html: '<p>@Grace said <a href="https://matrix.to/#/@grace:x">Grace</a></p>',
          },
        },
      },
    });
  });

  it('should keep two people with one name apart as the words around them change', () => {
    const { store } = setUp();
    setDraft(store, ADA, KEY, '@Gr');
    completeDraft(store, ADA, KEY, '@Grace ', { start: 7, end: 7 }, pillAt(grace, 0));
    setDraft(store, ADA, KEY, '@Grace and @Gr');
    completeDraft(
      store,
      ADA,
      KEY,
      '@Grace and @Grace ',
      { start: 18, end: 18 },
      pillAt(otherGrace, 11),
    );
    setDraft(store, ADA, KEY, 'hi @Grace and @Grace ', undefined, 0);
    setDraft(store, ADA, KEY, 'hi @Grace @Grace ', undefined, 13);
    setDraft(store, ADA, KEY, 'hi @Grace ', undefined, 3);
    expect(composer(store.getState(), ADA, KEY).pills).toEqual([pillAt(otherGrace, 3)]);
  });

  it('should drop a pill once its label is edited away', () => {
    const { store } = setUp();
    completeDraft(store, ADA, KEY, 'hi @Grace ', { start: 10, end: 10 }, pillAt(grace, 3));
    setDraft(store, ADA, KEY, 'hi @Gra');
    expect(composer(store.getState(), ADA, KEY).pills).toEqual([]);
  });

  it('should put a custom emoji picked for the draft at the caret as a pill', () => {
    const { store } = setUp();
    const cat = emotePill('mxc://x/cat', 'cat');
    insertIntoDraft(store, ADA, KEY, ':cat:', cat);
    expect(composer(store.getState(), ADA, KEY)).toMatchObject({
      draft: ':cat:',
      pills: [pillAt(cat, 0)],
    });
  });

  it('should move the pills after an emoji picked before them', () => {
    const { store } = setUp();
    completeDraft(store, ADA, KEY, 'hi @Grace', { start: 0, end: 0 }, pillAt(grace, 3));
    insertIntoDraft(store, ADA, KEY, '🎉 ');
    expect(composer(store.getState(), ADA, KEY).pills).toEqual([pillAt(grace, 6)]);
  });

  it.each(['bold', 'italic', 'strike', 'code'] as const)(
    'should keep a pill the toolbar makes %s around',
    (format) => {
      const { store } = setUp();
      completeDraft(store, ADA, KEY, 'hi @Grace', { start: 9, end: 9 }, pillAt(grace, 3));
      const marker = { bold: '**', italic: '_', strike: '~~', code: '`' }[format];
      spliceDraft(store, ADA, KEY, applyFormat('hi @Grace', { start: 3, end: 9 }, format));
      expect(composer(store.getState(), ADA, KEY)).toMatchObject({
        draft: `hi ${marker}@Grace${marker}`,
        pills: [pillAt(grace, 3 + marker.length)],
      });
    },
  );

  it('should send a mention made bold by the toolbar as a bold mention', () => {
    const { store, commands } = setUp();
    completeDraft(store, ADA, KEY, 'hi @Grace', { start: 9, end: 9 }, pillAt(grace, 3));
    spliceDraft(store, ADA, KEY, applyFormat('hi @Grace', { start: 3, end: 9 }, 'bold'));
    sendComposer(store, ADA, KEY);
    expect(commands()[0]).toMatchObject({
      data: {
        message: {
          data: {
            html: '<p>hi <strong><a href="https://matrix.to/#/@grace:x">Grace</a></strong></p>',
            mentions: { user_ids: ['@grace:x'] },
          },
        },
      },
    });
  });

  it('should keep a pill on a list item the toolbar continues', () => {
    const { store } = setUp();
    completeDraft(store, ADA, KEY, '- @Grace', { start: 8, end: 8 }, pillAt(grace, 2));
    const continued = continueBlock('- @Grace', '- @Grace\n');

    if (!continued) throw new Error('the list should continue');
    spliceDraft(store, ADA, KEY, continued);
    expect(composer(store.getState(), ADA, KEY).pills).toEqual([pillAt(grace, 2)]);
  });

  it('should keep a pill when the toolbar makes its line a bullet', () => {
    const { store } = setUp();
    completeDraft(store, ADA, KEY, '@Grace', { start: 6, end: 6 }, pillAt(grace, 0));
    spliceDraft(store, ADA, KEY, applyFormat('@Grace', { start: 0, end: 0 }, 'bullets'));
    expect(composer(store.getState(), ADA, KEY).pills).toEqual([pillAt(grace, 2)]);
  });

  it('should send a mention with whom it mentions and forget the pills', () => {
    const { store, commands } = setUp();
    completeDraft(store, ADA, KEY, 'hi @Grace', { start: 9, end: 9 }, pillAt(grace, 3));
    sendComposer(store, ADA, KEY);
    expect([commands()[0], composer(store.getState(), ADA, KEY).pills]).toMatchObject([
      { data: { message: { data: { mentions: { user_ids: ['@grace:x'] } } } } },
      undefined,
    ]);
  });
});

describe('commands from the composer', () => {
  it('should say an unknown command and keep the draft', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, '/frob');
    expect(sendComposer(store, ADA, KEY)).toBe(false);
    expect([
      commands(),
      composer(store.getState(), ADA, KEY).draft,
      store.getState().notices.map((n) => n.message),
    ]).toEqual([[], '/frob', [t('mobile.composer.command_unknown', { name: 'frob' })]]);
  });

  it('should invite with /invite and empty the draft', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, '/invite @kai:x');
    sendComposer(store, ADA, KEY);
    expect([commands(), composer(store.getState(), ADA, KEY).draft]).toEqual([
      [{ type: 'Invite', data: { account: ADA, room_id: ROOM, user_id: '@kai:x' } }],
      '',
    ]);
  });

  it('should open the room /join joined', async () => {
    const { store, succeed } = setUp();
    const joined = jest.fn();
    setDraft(store, ADA, KEY, '/join #rust:x');
    sendComposer(store, ADA, KEY, { joined });
    store.engine.emit({ type: 'RoomJoined', data: { request: 1, room_id: '!rust:x' } });
    succeed(0);
    await flush();
    expect(joined).toHaveBeenCalledWith('!rust:x');
  });

  it('should leave the room’s screen once /leave worked', async () => {
    const { store, succeed } = setUp();
    const left = jest.fn();
    setDraft(store, ADA, KEY, '/leave');
    sendComposer(store, ADA, KEY, { left });
    succeed(0);
    await flush();
    expect(left).toHaveBeenCalled();
  });

  it('should say why a command failed', async () => {
    const { store, fail } = setUp();
    setDraft(store, ADA, KEY, '/topic Hi');
    sendComposer(store, ADA, KEY);
    fail(0, 'forbidden');
    await flush();
    expect(store.getState().notices.map((n) => n.message)).toEqual([
      t('mobile.composer.command_failed', { error: 'forbidden' }),
    ]);
  });
});

describe('scheduling from the composer', () => {
  it('should hand the draft to the server and empty the composer', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, 'later');
    expect(scheduleComposer(store, ADA, KEY, 1_000)).toBe(true);
    expect([commands().map((c) => c.type), composer(store.getState(), ADA, KEY).draft]).toEqual([
      ['Schedule'],
      '',
    ]);
  });

  it('should give the text back when the server refused it', async () => {
    const { store, fail } = setUp();
    setDraft(store, ADA, KEY, 'later');
    scheduleComposer(store, ADA, KEY, 1_000);
    fail(0, 'unsupported');
    await flush();
    expect(composer(store.getState(), ADA, KEY).draft).toBe('later');
  });

  it('should give the whole composer back when the server refused it: pills and reply too', async () => {
    const { store, fail } = setUp();
    const grace = pillAt(userPill('@grace:x', 'Grace'), 6);
    startReply(store, ADA, KEY, message('$q', 'when?'));
    completeDraft(store, ADA, KEY, 'later @Grace', { start: 12, end: 12 }, grace);
    scheduleComposer(store, ADA, KEY, 1_000);
    fail(0, 'unsupported');
    await flush();
    const after = composer(store.getState(), ADA, KEY);
    expect([after.draft, after.pills, after.replyTo?.event_id]).toEqual([
      'later @Grace',
      [grace],
      '$q',
    ]);
  });

  it('should not schedule attachments', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, 'x');
    addAttachments(store, ADA, KEY, [pdf]);
    expect([scheduleComposer(store, ADA, KEY, 1_000), commands()]).toEqual([false, []]);
  });

  it('should not run a command that acts when a time is picked, keeping the draft and saying why', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, '/leave');
    expect([
      scheduleComposer(store, ADA, KEY, 1_000),
      commands(),
      composer(store.getState(), ADA, KEY).draft,
      store.getState().notices.map((n) => n.message),
    ]).toEqual([false, [], '/leave', [t('mobile.composer.schedule_command')]]);
  });

  it('should schedule a command that makes a message', () => {
    const { store, commands } = setUp();
    setDraft(store, ADA, KEY, '/me waves');
    scheduleComposer(store, ADA, KEY, 1_000);
    expect(commands()[0]).toMatchObject({
      type: 'Schedule',
      data: { message: { type: 'Emote', data: { body: 'waves' } } },
    });
  });
});
