import { type ComposerState, EMPTY_COMPOSER, type PendingAttachment } from '../composer';
import {
  canSend,
  excerpt,
  formatDuration,
  insertAt,
  pathFromUri,
  planSend,
  uploadName,
  voiceMessage,
  withAttachments,
} from '../outgoing';
import { roomKey } from '../timeline';
import { ADA, message } from '../../test/fixtures';

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
          message: { type: 'Text', data: { body: 'hello', html: null } },
        },
      },
    ]);
  });

  it('should send formatted text with its html', () => {
    const [command] = planSend({ ...EMPTY_COMPOSER, draft: '**hi**' }, ADA, KEY).commands;
    expect(command?.type === 'Send' && command.data.message).toEqual({
      type: 'Text',
      data: { body: 'hi', html: '<p><strong>hi</strong></p>' },
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
        data: { account: ADA, key: KEY, event_id: '$mine', body: 'helo!', html: null },
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
