// What a composer sends, decided as plain data: which commands, in which order, and the
// composer left behind (the desktop's `Composer::take_outgoing`, `take_edit` and `send_from`).

import type {
  AccountId,
  Command,
  Message,
  Outgoing,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import { type ComposerState, EMPTY_COMPOSER, endEdit, type PendingAttachment } from './composer';
import { outgoingText } from './format';

/** The commands a send makes, and the composer after it. */
export interface SendPlan {
  commands: Command[];
  composer: ComposerState;
}

/**
 * Everything a composer sends, text first, then each attachment in the order picked; the reply,
 * if any, goes with the first message (the engine sends files as they are, so a reply rides on
 * the text). HTML only when the text carries formatting. An edit sends `Edit` instead, unless the
 * text is empty or unchanged; either way the draft set aside for it comes back.
 */
export function planSend(composer: ComposerState, account: AccountId, key: TimelineKey): SendPlan {
  if (composer.editing) {
    const { body, html } = outgoingText(composer.draft);
    const original = outgoingText(composer.setAside?.original ?? '');
    const changed = body !== original.body || html !== original.html;

    const commands: Command[] =
      body !== '' && changed
        ? [
            {
              type: 'Edit',
              data: { account, key, event_id: composer.editing.event_id, body, html },
            },
          ]
        : [];

    // Sending an edit of a message drops any kept edit of it.
    const editedId = composer.editing.event_id;
    const kept = composer.kept?.filter((edit) => edit.message.event_id !== editedId);

    return { commands, composer: endEdit({ ...composer, kept }) };
  }

  const messages: Outgoing[] = [];
  const text = outgoingText(composer.draft);

  if (text.body !== '') messages.push({ type: 'Text', data: text });

  for (const attachment of composer.attachments ?? [])
    messages.push({ type: 'File', data: { type: 'Path', data: attachment.path } });
  let replyTo = composer.replyTo?.event_id ?? null;

  const commands = messages.map((message): Command => {
    const command: Command = { type: 'Send', data: { account, key, reply_to: replyTo, message } };
    replyTo = null;

    return command;
  });

  return {
    commands,
    composer: commands.length > 0 ? { ...EMPTY_COMPOSER, kept: composer.kept } : composer,
  };
}

/** Whether a composer has anything to send (or an edit to save). */
export function canSend(composer: ComposerState): boolean {
  if (composer.editing) return outgoingText(composer.draft).body !== '';

  return outgoingText(composer.draft).body !== '' || (composer.attachments?.length ?? 0) > 0;
}

/** A voice memo as the message that sends it, with its waveform (none when empty). */
export function voiceMessage(path: string, durationMs: number, waveform: number[] = []): Outgoing {
  return {
    type: 'Voice',
    data: {
      upload: { type: 'Path', data: path },
      duration_ms: Math.max(0, Math.round(durationMs)),
      waveform,
    },
  };
}

/** How long an excerpt of the answered message shows above the field, as on the desktop. */
const EXCERPT = 140;

/** The words of a message a reply quotes: its text, its file's name, its poll's question. */
export function excerpt(message: Message): string {
  const content = message.content;

  switch (content.type) {
    case 'Text':
      return [...content.data.body].slice(0, EXCERPT).join('');
    case 'Attachment':
      return content.data.name;
    case 'Poll':
      return content.data.question;
    default:
      return '';
  }
}

/** A plain path from a `file://` URI, as `Upload::Path` takes it. */
export function pathFromUri(uri: string): string {
  const path = uri.replace(/^file:\/\//, '');

  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

/** The last part of a path. */
export function fileName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

/**
 * The name to send a picked file under: the name the picker gave, with the extension of the file
 * actually on disk (a photo named `IMG_1.HEIC` may arrive as JPEG), or the file's own name.
 */
export function uploadName(path: string, given: string | null | undefined): string {
  const own = fileName(path);

  if (!given) return own;
  const extension = /\.[^./]+$/.exec(own)?.[0] ?? '';
  const stem = given.replace(/\.[^./]+$/, '').replace(/[/\\]/g, '_');

  return stem === '' ? own : stem + extension;
}

/** Adds picked files to a composer, each once, after those picked before. */
export function withAttachments(
  composer: ComposerState,
  picked: readonly PendingAttachment[],
): ComposerState {
  const attachments = [...(composer.attachments ?? [])];

  for (const attachment of picked) {
    if (!attachments.some((a) => a.path === attachment.path)) attachments.push(attachment);
  }

  return { ...composer, attachments };
}

/** `text` with `insert` put in place of the selection; the caret goes after it. */
export function insertAt(
  text: string,
  selection: { start: number; end: number } | undefined,
  insert: string,
) {
  const start = Math.min(selection?.start ?? text.length, text.length);
  const end = Math.min(Math.max(selection?.end ?? text.length, start), text.length);
  const caret = start + insert.length;

  return {
    text: text.slice(0, start) + insert + text.slice(end),
    selection: { start: caret, end: caret },
  };
}

/** `m:ss`, for a recording's length. */
export function formatDuration(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));

  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
