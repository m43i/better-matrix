// The preview's gallery: the attachments a sender posted together, which the full-screen
// preview steps through. Matrix carries one attachment per message, so a batch of photos
// arrives as a run of attachment messages; the run is the batch (the desktop's rule).

import type {
  Attachment,
  MediaRef,
  MediaSize,
  Message,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';

import { attachmentPreview } from './messages';
import { GROUP_WINDOW_MS } from './plan';

/** A sender's next attachment comes within this of the one before to join its batch, as the
 * timeline groups messages under one header (`plan.ts`, the desktop's `continues_group`). */
export const BATCH_WINDOW_MS = GROUP_WINDOW_MS;

/** Whether the preview can show `attachment`: an image with a picture, a video, an audio clip. */
export function previewable(attachment: Attachment): boolean {
  switch (attachment.kind) {
    case 'Image':
      return attachmentPreview(attachment) !== null;
    case 'Video':
    case 'Audio':
      return attachment.media !== null;
    case 'File':
      return false;
  }
}

function attachmentOf(message: Message): Attachment | null {
  return message.content.type === 'Attachment' ? message.content.data : null;
}

/**
 * Whether `later` follows `earlier` in one batch: the same sender, both attachments, within the
 * batch window, and `later` no reply.
 */
function sameBatch(earlier: Message, later: Message): boolean {
  return (
    earlier.sender_id === later.sender_id &&
    attachmentOf(earlier) !== null &&
    attachmentOf(later) !== null &&
    later.timestamp_ms - earlier.timestamp_ms < BATCH_WINDOW_MS &&
    later.reply_to === null
  );
}

/**
 * The message `step` (±1) entries along from `index`, past the entries the timeline doesn't draw
 * (read markers, unknown events); `null` at anything else that isn't a message, or the end.
 */
function neighbour(entries: readonly TimelineEntry[], index: number, step: 1 | -1) {
  for (let i = index + step; i >= 0 && i < entries.length; i += step) {
    const kind = entries[i]?.kind;

    if (kind?.type === 'ReadMarker' || kind?.type === 'Other') continue;

    return kind?.type === 'Message' ? { message: kind.data, index: i } : null;
  }

  return null;
}

/**
 * The batch the attachment message `eventId` belongs to, oldest first: the run of attachment
 * messages around it from its sender, each within the batch window of the one before and none a
 * reply. A date divider, a room event or another message ends the run. Empty when `eventId`
 * isn't an attachment message in `entries`.
 */
export function attachmentBatch(entries: readonly TimelineEntry[], eventId: string): Message[] {
  const at = entries.findIndex(
    (e) => e.kind.type === 'Message' && e.kind.data.event_id === eventId,
  );

  const opened = entries[at]?.kind;

  if (opened?.type !== 'Message' || !attachmentOf(opened.data)) return [];
  const start = opened.data;

  /** The run from the opened message `step` along, nearest first. */
  const extend = (step: 1 | -1) => {
    const run: Message[] = [];
    let current = start;

    for (
      let next = neighbour(entries, at, step);
      next;
      next = neighbour(entries, next.index, step)
    ) {
      const linked = step < 0 ? sameBatch(next.message, current) : sameBatch(current, next.message);

      if (!linked) break;
      run.push(next.message);
      current = next.message;
    }

    return run;
  };

  return [...extend(-1).reverse(), start, ...extend(1)];
}

/**
 * What the preview opened at `eventId` steps through: the previewable attachments of its batch,
 * the opened one always among them. Two or more get the preview's strip of thumbnails.
 */
export function previewGallery(entries: readonly TimelineEntry[], eventId: string): Message[] {
  return attachmentBatch(entries, eventId).filter((message) => {
    const attachment = attachmentOf(message);

    return message.event_id === eventId || (attachment !== null && previewable(attachment));
  });
}

/** Whether a gallery shows its strip of thumbnails: when there is more than one to show. */
export function showsStrip(gallery: readonly Message[]): boolean {
  return gallery.length > 1;
}

/** The index `step` along from `index` in a gallery of `count`, round past either end. */
export function stepGallery(index: number, step: number, count: number): number {
  if (count <= 0) return 0;

  return (((index + step) % count) + count) % count;
}

/**
 * The picture a gallery thumbnail shows, as the timeline fetches it (so it is usually there
 * already): an image's preview, a video's poster; `null` draws the kind's icon instead.
 */
export function thumbnailOf(attachment: Attachment): { media: MediaRef; size: MediaSize } | null {
  if (attachment.kind === 'Image') return attachmentPreview(attachment);

  if (attachment.kind === 'Video' && attachment.thumbnail)
    return { media: attachment.thumbnail, size: { type: 'Original' } };

  return null;
}
