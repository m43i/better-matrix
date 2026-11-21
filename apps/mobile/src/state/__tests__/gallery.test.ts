import type { Attachment, Message, Quote, TimelineEntry } from '@better-matrix/matrix-native/types';

import {
  galleryAround,
  attachmentBatch,
  BATCH_WINDOW_MS,
  previewable,
  previewGallery,
  showsStrip,
  stepGallery,
  thumbnailOf,
} from '../gallery';
import { ADA, messageEntry, person, roomEventEntry } from '../../test/fixtures';

const T = 1_700_000_000_000;

function attachment(kind: Attachment['kind'], change: Partial<Attachment> = {}): Attachment {
  return {
    kind,
    name: `${kind.toLowerCase()}.bin`,
    size: 1000,
    media: `mxc://example.org/${kind}`,
    thumbnail: null,
    dimensions: null,
    audio: null,
    ...change,
  };
}

/** An attachment message from Grace `minutes` after T. */
function sent(
  id: string,
  minutes: number,
  kind: Attachment['kind'] = 'Image',
  change: Partial<Message> = {},
): TimelineEntry {
  return messageEntry(id, {
    content: { type: 'Attachment', data: attachment(kind) },
    timestamp_ms: T + minutes * 60_000,
    ...change,
  });
}

const ids = (messages: Message[]) => messages.map((m) => m.event_id);

const quote: Quote = { event_id: 'a', sender_name: 'Grace', body: 'image.bin', call: null };

describe('attachmentBatch', () => {
  it('should gather a run of attachments from the same sender', () => {
    const entries = [sent('a', 0), sent('b', 1, 'Video'), sent('c', 2, 'Audio')];

    expect(ids(attachmentBatch(entries, 'b'))).toEqual(['a', 'b', 'c']);
  });

  it('should end the run at a text message', () => {
    const entries = [
      sent('a', 0),
      messageEntry('text', { timestamp_ms: T + 60_000 }),
      sent('b', 2),
    ];

    expect(ids(attachmentBatch(entries, 'b'))).toEqual(['b']);
  });

  it('should end the run at another sender’s attachment', () => {
    const entries = [sent('a', 0), sent('b', 1, 'Image', { sender_id: ADA }), sent('c', 2)];

    expect(ids(attachmentBatch(entries, 'a'))).toEqual(['a']);
  });

  it('should end the run at a gap longer than five minutes', () => {
    const entries = [
      sent('a', 0),
      sent('b', BATCH_WINDOW_MS / 60_000 - 1),
      messageEntry('c', {
        content: { type: 'Attachment', data: attachment('Image') },
        timestamp_ms: T + 2 * BATCH_WINDOW_MS - 60_000,
      }),
    ];

    expect(ids(attachmentBatch(entries, 'a'))).toEqual(['a', 'b']);
  });

  it('should end the run at a thread’s root, above its replies', () => {
    const entries = [sent('root', 0), sent('a', 1), sent('b', 2)];

    expect([
      ids(attachmentBatch(entries, 'a', 'root')),
      ids(attachmentBatch(entries, 'root', 'root')),
    ]).toEqual([['a', 'b'], ['root']]);
  });

  it('should end the run at a reply', () => {
    const entries = [sent('a', 0), sent('b', 1, 'Image', { reply_to: quote }), sent('c', 2)];

    expect([ids(attachmentBatch(entries, 'a')), ids(attachmentBatch(entries, 'c'))]).toEqual([
      ['a'],
      ['b', 'c'],
    ]);
  });

  it('should end the run at a date divider or a room event', () => {
    const entries: TimelineEntry[] = [
      sent('a', 0),
      { id: 'day', kind: { type: 'DateDivider', data: T } },
      sent('b', 1),
      roomEventEntry('joined', person(ADA), { type: 'Joined' }),
      sent('c', 2),
    ];

    expect(ids(attachmentBatch(entries, 'b'))).toEqual(['b']);
  });

  it('should run on past entries the timeline does not draw', () => {
    const entries: TimelineEntry[] = [
      sent('a', 0),
      { id: 'read-marker', kind: { type: 'ReadMarker' } },
      sent('b', 1),
      { id: 'other', kind: { type: 'Other' } },
      sent('c', 2),
    ];

    expect(ids(attachmentBatch(entries, 'c'))).toEqual(['a', 'b', 'c']);
  });

  it('should find no batch for a message that is not an attachment', () => {
    expect(attachmentBatch([messageEntry('text')], 'text')).toEqual([]);
  });
});

describe('previewGallery', () => {
  it('should leave out the files the preview cannot show', () => {
    const entries = [sent('a', 0), sent('pdf', 1, 'File'), sent('c', 2, 'Video')];

    expect(ids(previewGallery(entries, 'a'))).toEqual(['a', 'c']);
  });

  it('should leave out a video that has no file', () => {
    const entries = [
      sent('a', 0),
      messageEntry('v', {
        content: { type: 'Attachment', data: attachment('Video', { media: null }) },
        timestamp_ms: T + 60_000,
      }),
    ];

    expect(ids(previewGallery(entries, 'a'))).toEqual(['a']);
  });

  it('should make a lone attachment a gallery of one, which shows no strip', () => {
    const gallery = previewGallery([sent('a', 0)], 'a');

    expect([ids(gallery), showsStrip(gallery)]).toEqual([['a'], false]);
  });

  it('should show the strip for two or more', () => {
    expect(showsStrip(previewGallery([sent('a', 0), sent('b', 1)], 'a'))).toBe(true);
  });
});

describe('previewable', () => {
  it('should not preview a plain file', () => {
    expect(previewable(attachment('File'))).toBe(false);
  });

  it('should preview an audio clip that has a file', () => {
    expect(previewable(attachment('Audio'))).toBe(true);
  });
});

describe('stepGallery', () => {
  it('should step forward round past the last', () => {
    expect(stepGallery(2, 1, 3)).toBe(0);
  });

  it('should step back round past the first', () => {
    expect(stepGallery(0, -1, 3)).toBe(2);
  });
});

describe('thumbnailOf', () => {
  it('should show an image by its own thumbnail when it has one', () => {
    expect(thumbnailOf(attachment('Image', { thumbnail: 'mxc://example.org/thumb' }))).toEqual({
      media: 'mxc://example.org/thumb',
      size: { type: 'Original' },
    });
  });

  it('should show a video by its poster', () => {
    expect(thumbnailOf(attachment('Video', { thumbnail: 'mxc://example.org/poster' }))).toEqual({
      media: 'mxc://example.org/poster',
      size: { type: 'Original' },
    });
  });

  it('should show an audio clip by its icon', () => {
    expect(thumbnailOf(attachment('Audio'))).toBeNull();
  });
});

describe('galleryAround', () => {
  it('should keep the item shown when the opened one is redacted', () => {
    const redacted = messageEntry('a', { content: { type: 'Redacted' }, timestamp_ms: T });
    const entries = [redacted, sent('b', 1), sent('c', 2)];

    expect(ids(galleryAround(entries, 'c', 'a'))).toEqual(['b', 'c']);
  });
});
