import { type ComposerState, EMPTY_COMPOSER, type PendingAttachment } from '../composer';
import { pillAt, spoilerPill, userPill } from '../pills';
import {
  canSend,
  excerpt,
  formatDuration,
  insertAt,
  MAX_SCHEDULE_MS,
  pathFromUri,
  planSchedule,
  scheduleProblem,
  planSend,
  uploadName,
  voiceMessage,
  withAttachments,
} from '../outgoing';
import { roomKey } from '../timeline';
import { ADA, EVERY_PRIVATE_USE, message } from '../../test/fixtures';

/** `m.mentions` for a message that mentions nobody. */
const NOBODY = { user_ids: [], room: false };

const KEY = roomKey('!r:x');

function file(path: string): PendingAttachment {
  return { path, name: path.slice(path.lastIndexOf('/') + 1), kind: 'file', uri: `file://${path}` };
}

function editing(draft: string, original: string): ComposerState {
  return {
    ...EMPTY_COMPOSER,
    draft,
    editing: message('$mine'),
    setAside: { draft: 'half a thought', replyTo: null, attachments: [file('/a.png')], original },
  };
}

describe('planSend', () => {
  it('should send plain text without html', () => {
    const plan = planSend({ ...EMPTY_COMPOSER, draft: 'hello' }, ADA, KEY);
    expect(plan.commands).toEqual([
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
  });

  it('should send formatted text with its html', () => {
    const [command] = planSend({ ...EMPTY_COMPOSER, draft: '**hi**' }, ADA, KEY).commands;
    expect(command?.type === 'Send' && command.data.message).toEqual({
      type: 'Text',
      data: { body: 'hi', html: '<p><strong>hi</strong></p>', mentions: NOBODY },
    });
  });

  it('should send the attachments after the text, in the order picked', () => {
    const plan = planSend(
      { ...EMPTY_COMPOSER, draft: 'look', attachments: [file('/a.png'), file('/b.pdf')] },
      ADA,
      KEY,
    );

    expect(plan.commands.map((c) => c.type === 'Send' && c.data.message.type)).toEqual([
      'Text',
      'File',
      'File',
    ]);
    const last = plan.commands[2];
    expect(last?.type === 'Send' && last.data.message).toEqual({
      type: 'File',
      data: { type: 'Path', data: '/b.pdf' },
    });
  });

  it('should send the reply with the first message only', () => {
    const plan = planSend(
      { ...EMPTY_COMPOSER, draft: 'yes', replyTo: message('$q'), attachments: [file('/a.png')] },
      ADA,
      KEY,
    );

    expect(plan.commands.map((c) => c.type === 'Send' && c.data.reply_to)).toEqual(['$q', null]);
  });

  it('should leave an empty composer after sending', () => {
    expect(
      planSend({ ...EMPTY_COMPOSER, draft: 'hi', replyTo: message('$q') }, ADA, KEY).composer,
    ).toEqual(EMPTY_COMPOSER);
  });

  it('should send nothing for an empty draft', () => {
    expect(planSend({ ...EMPTY_COMPOSER, draft: '  ' }, ADA, KEY).commands).toEqual([]);
  });

  it('should send a changed edit as an edit of the message', () => {
    expect(planSend(editing('helo!', 'helo'), ADA, KEY).commands).toEqual([
      {
        type: 'Edit',
        data: {
          account: ADA,
          key: KEY,
          event_id: '$mine',
          body: 'helo!',
          html: null,
          mentions: NOBODY,
        },
      },
    ]);
  });

  it('should not send an unchanged edit', () => {
    expect(planSend(editing('same', 'same'), ADA, KEY).commands).toEqual([]);
  });

  it('should not send an emptied edit', () => {
    expect(planSend(editing('', 'x'), ADA, KEY).commands).toEqual([]);
  });

  it('should bring the draft back after an edit', () => {
    const after = planSend(editing('helo!', 'helo'), ADA, KEY).composer;
    expect([after.draft, after.attachments?.length, after.editing]).toEqual([
      'half a thought',
      1,
      null,
    ]);
  });
});

describe('canSend', () => {
  it('should allow sending attachments without text', () => {
    expect(canSend({ ...EMPTY_COMPOSER, attachments: [file('/a')] })).toBe(true);
  });

  it('should not allow sending only spaces', () => {
    expect(canSend({ ...EMPTY_COMPOSER, draft: ' \n' })).toBe(false);
  });
});

describe('withAttachments', () => {
  it('should keep a file picked twice once', () => {
    expect(withAttachments(EMPTY_COMPOSER, [file('/a'), file('/a')]).attachments).toHaveLength(1);
  });
});

describe('voiceMessage', () => {
  it('should send the recording’s waveform with it', () => {
    const message = voiceMessage('/v.m4a', 2000, [10, 900]);
    expect(message.type === 'Voice' && message.data.waveform).toEqual([10, 900]);
  });

  it('should send the recording with its whole milliseconds', () => {
    expect(voiceMessage('/v.m4a', 1234.6)).toEqual({
      type: 'Voice',
      data: { upload: { type: 'Path', data: '/v.m4a' }, duration_ms: 1235, waveform: [] },
    });
  });
});

describe('excerpt', () => {
  it('should quote a text message up to 140 characters', () => {
    expect(excerpt(message('$1', 'x'.repeat(200)))).toHaveLength(140);
  });

  it('should quote a file by its name', () => {
    const photo = {
      ...message('$1'),
      content: {
        type: 'Attachment' as const,
        data: {
          kind: 'Image' as const,
          name: 'cat.png',
          size: null,
          media: null,
          thumbnail: null,
          dimensions: null,
          audio: null,
        },
      },
    };

    expect(excerpt(photo)).toBe('cat.png');
  });
});

describe('pathFromUri', () => {
  it('should strip the scheme and decode the path', () => {
    expect(pathFromUri('file:///data/cache/My%20File%23.pdf')).toBe('/data/cache/My File#.pdf');
  });
});

describe('uploadName', () => {
  it('should keep the picked name with the extension on disk', () => {
    expect(uploadName('/cache/1F2E.jpg', 'IMG_0001.HEIC')).toBe('IMG_0001.jpg');
  });

  it('should fall back to the name on disk', () => {
    expect(uploadName('/cache/report.pdf', null)).toBe('report.pdf');
  });
});

describe('insertAt', () => {
  it('should put the text at the caret and the caret after it', () => {
    expect(insertAt('ab', { start: 1, end: 1 }, '🎉')).toEqual({
      text: 'a🎉b',
      selection: { start: 3, end: 3 },
    });
  });

  it('should put the text at the end without a caret', () => {
    expect(insertAt('ab', undefined, '!').text).toBe('ab!');
  });

  it('should replace a selection', () => {
    expect(insertAt('abc', { start: 1, end: 2 }, 'X').text).toBe('aXc');
  });
});

describe('formatDuration', () => {
  it('should show minutes and seconds', () => {
    expect(formatDuration(65_400)).toBe('1:05');
  });
});

describe('planSend with pills and commands', () => {
  const grace = userPill('@grace:x', 'Grace');

  it('should send whom the pills mention', () => {
    const plan = planSend(
      { ...EMPTY_COMPOSER, draft: 'hi @Grace', pills: [pillAt(grace, 3)] },
      ADA,
      KEY,
    );

    expect(plan.commands[0]).toMatchObject({
      data: { message: { data: { mentions: { user_ids: ['@grace:x'], room: false } } } },
    });
  });

  it('should send an edit with whom it mentions', () => {
    const composer = { ...editing('hi @Grace', 'hi'), pills: [pillAt(grace, 3)] };
    expect(planSend(composer, ADA, KEY).commands[0]).toMatchObject({
      type: 'Edit',
      data: { mentions: { user_ids: ['@grace:x'] } },
    });
  });

  it('should send an emote for /me with the reply', () => {
    const composer = { ...EMPTY_COMPOSER, draft: '/me waves', replyTo: message('$q') };
    expect(planSend(composer, ADA, KEY).commands).toEqual([
      {
        type: 'Send',
        data: {
          account: ADA,
          key: KEY,
          reply_to: '$q',
          message: { type: 'Emote', data: { body: 'waves', html: null, mentions: NOBODY } },
        },
      },
    ]);
  });

  it('should send a command in the draft’s place, keeping the reply and attachments', () => {
    const composer = {
      ...EMPTY_COMPOSER,
      draft: '/topic Release',
      replyTo: message('$q'),
      attachments: [file('/a.png')],
    };

    const plan = planSend(composer, ADA, KEY);
    expect([
      plan.commands.map((c) => c.type),
      plan.composer.draft,
      plan.composer.replyTo,
      plan.after,
    ]).toEqual([['ChangeRoom'], '', message('$q'), 'stay']);
    expect(plan.composer.attachments).toEqual([file('/a.png')]);
  });

  it('should send nothing for an unknown command and say so', () => {
    const plan = planSend({ ...EMPTY_COMPOSER, draft: '/frob' }, ADA, KEY);
    expect([plan.commands, plan.problem, plan.composer.draft]).toEqual([
      [],
      { type: 'unknown', name: 'frob' },
      '/frob',
    ]);
  });
});

describe('a draft whose pills can’t be kept', () => {
  /** Every private-use character, then a spoiler: no character is left to stand for it. */
  const draft = `${EVERY_PRIVATE_USE} secret`;
  const pills = [pillAt(spoilerPill('secret'), EVERY_PRIVATE_USE.length + 1)];

  it('should send nothing, keep the draft and say why', () => {
    const composer = { ...EMPTY_COMPOSER, draft, pills };
    const plan = planSend(composer, ADA, KEY);
    expect([plan.commands, plan.problem, plan.composer]).toEqual([[], { type: 'pills' }, composer]);
  });

  it('should save no edit, keeping the edit under way', () => {
    const composer = { ...editing(draft, 'hi'), pills };
    const plan = planSend(composer, ADA, KEY);
    expect([plan.commands, plan.problem, plan.composer]).toEqual([[], { type: 'pills' }, composer]);
  });

  it('should not be scheduled', () => {
    const composer = { ...EMPTY_COMPOSER, draft, pills };
    expect([scheduleProblem(composer, ADA, KEY), planSchedule(composer, ADA, KEY, 1)]).toEqual([
      { type: 'pills' },
      null,
    ]);
  });

  it('should send nothing for /me either', () => {
    const composer = {
      ...EMPTY_COMPOSER,
      draft: `/me ${draft}`,
      pills: [pillAt(spoilerPill('secret'), EVERY_PRIVATE_USE.length + 5)],
    };

    expect(planSend(composer, ADA, KEY).problem).toEqual({ type: 'pills' });
  });
});

describe('planSchedule', () => {
  it('should ask the server to send the text later, with its reply', () => {
    const composer = { ...EMPTY_COMPOSER, draft: 'later', replyTo: message('$q') };
    expect(planSchedule(composer, ADA, KEY, 60_000)?.commands).toEqual([
      {
        type: 'Schedule',
        data: {
          account: ADA,
          key: KEY,
          reply_to: '$q',
          message: { type: 'Text', data: { body: 'later', html: null, mentions: NOBODY } },
          delay_ms: 60_000,
        },
      },
    ]);
  });

  it('should empty the composer it scheduled', () => {
    expect(planSchedule({ ...EMPTY_COMPOSER, draft: 'later' }, ADA, KEY, 1)?.composer).toEqual(
      EMPTY_COMPOSER,
    );
  });

  it('should hold a message at most 30 days', () => {
    const plan = planSchedule({ ...EMPTY_COMPOSER, draft: 'x' }, ADA, KEY, 2 * MAX_SCHEDULE_MS);
    expect(plan?.commands[0]).toMatchObject({ data: { delay_ms: MAX_SCHEDULE_MS } });
  });

  it('should not schedule attachments', () => {
    const composer = { ...EMPTY_COMPOSER, draft: 'x', attachments: [file('/a.png')] };
    expect([scheduleProblem(composer, ADA, KEY), planSchedule(composer, ADA, KEY, 1)]).toEqual([
      { type: 'attachments' },
      null,
    ]);
  });

  it('should not schedule a command that isn’t a message', () => {
    const composer = { ...EMPTY_COMPOSER, draft: '/leave' };
    expect([scheduleProblem(composer, ADA, KEY), planSchedule(composer, ADA, KEY, 1)]).toEqual([
      { type: 'command' },
      null,
    ]);
  });

  it('should not schedule a command that doesn’t exist', () => {
    expect(scheduleProblem({ ...EMPTY_COMPOSER, draft: '/frob' }, ADA, KEY)).toEqual({
      type: 'unknown',
      name: 'frob',
    });
  });

  it.each([
    '/me waves',
    '/plain *x*',
    '/spoiler the end',
    '/shrug',
    '/tableflip',
    '/unflip',
    '/lenny',
    '//path',
  ])('should schedule %j, a command that makes a message', (draft) => {
    expect(scheduleProblem({ ...EMPTY_COMPOSER, draft }, ADA, KEY)).toBeNull();
  });
});

describe('excerpt of other kinds', () => {
  it('should quote an emote with its sender', () => {
    const emote = {
      ...message('$e'),
      content: { type: 'Emote' as const, data: { body: 'waves', html: null } },
    };

    expect(excerpt(emote)).toBe('* Grace waves');
  });

  it('should quote a place by its name', () => {
    const place = {
      ...message('$p'),
      content: {
        type: 'Location' as const,
        data: { geo_uri: 'geo:1,2', description: 'Home', map: null },
      },
    };

    expect(excerpt(place)).toBe('Home');
  });
});
