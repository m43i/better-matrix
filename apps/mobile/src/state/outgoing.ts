// What a composer sends, decided as plain data: which commands, in which order, and the
// composer left behind (the desktop's `Composer::take_outgoing`, `take_edit` and `send_from`).

import type {
  AccountId,
  Command,
  Message,
  Outgoing,
  TimelineKey,
} from '@better-matrix/matrix-native/types';

import type { Key } from '../i18n';
import { type ComposerState, emptied, endEdit, type PendingAttachment } from './composer';
import { hasText as draftHasText, outgoingText } from './format';
import { coordinatesText } from './geo';
import { spoilerSafeText } from './html';
import { slashOf } from './slash';

/** The commands a send makes, and the composer after it. */
export interface SendPlan {
  commands: Command[];
  composer: ComposerState;
  /** What to tell the user instead, when the draft was a command that can't be done. */
  problem?: SendProblem;
  /**
   * Where the app goes once the commands worked (a room joined, a room left), or what it opens
   * (`meeting`: the widget screen starting a meeting, with no command of its own).
   */
  after?: 'stay' | 'joined' | 'left' | 'meeting';
}

/**
 * Why a draft didn't send: a command that doesn't exist, lacks what it needs, or would show a
 * spoiler in what it sets; or a draft holding so many private-use characters that its pills
 * can't be kept (`outgoingText`).
 */
export type SendProblem =
  | { type: 'unknown'; name: string }
  | { type: 'usage'; name: string; args: Key }
  | { type: 'spoiler'; name: string }
  | { type: 'pills' }
  /** `/meet` where the room can't start a meeting. */
  | { type: 'no_meetings' };

/** The text message a draft sends: its own text, or what its command makes of it. */
type DraftMessage =
  | { type: 'message'; message: Outgoing | null }
  | { type: 'command'; command: Command; after: 'stay' | 'joined' | 'left' }
  | { type: 'meet' }
  | { type: 'problem'; problem: SendProblem };

function draftMessage(composer: ComposerState, account: AccountId, key: TimelineKey): DraftMessage {
  const pills = composer.pills ?? [];
  const slash = slashOf(composer.draft, pills, { account, roomId: key.room_id });

  if (slash === null) {
    const text = outgoingText(composer.draft, pills);

    if (!text) return { type: 'problem', problem: { type: 'pills' } };

    return { type: 'message', message: text.body !== '' ? { type: 'Text', data: text } : null };
  }

  switch (slash.type) {
    case 'message':
      return { type: 'message', message: slash.message };
    case 'command':
      return { type: 'command', command: slash.command, after: slash.after };
    case 'meet':
      return slash;
    case 'unknown':
    case 'usage':
    case 'spoiler':
    case 'pills':
      return { type: 'problem', problem: slash };
  }
}

/**
 * Everything a composer sends, text first, then each attachment in the order picked; the reply,
 * if any, goes with the first message (the engine sends files as they are, so a reply rides on
 * the text). HTML only when the text carries formatting or pills; `m.mentions` names whom its
 * pills mention. A draft starting with a command does what the command says (`state/slash.ts`).
 * An edit sends `Edit` instead, unless the text is empty or unchanged; either way the draft set
 * aside for it comes back.
 */
export function planSend(
  composer: ComposerState,
  account: AccountId,
  key: TimelineKey,
  /** Whether the room can start a meeting (`RoomSummary::can_start_meeting`), for `/meet`. */
  canStartMeeting = false,
): SendPlan {
  if (composer.editing) {
    const text = outgoingText(composer.draft, composer.pills ?? []);

    // The edit stays under way, its draft as it is, and the user is told why.
    if (!text) return { commands: [], composer, problem: { type: 'pills' } };
    const { body, html, mentions } = text;

    const original = outgoingText(
      composer.setAside?.original ?? '',
      composer.setAside?.originalPills ?? [],
    );

    const changed = original === null || body !== original.body || html !== original.html;

    const commands: Command[] =
      body !== '' && changed
        ? [
            {
              type: 'Edit',
              data: { account, key, event_id: composer.editing.event_id, body, html, mentions },
            },
          ]
        : [];

    // Sending an edit of a message drops any kept edit of it.
    const editedId = composer.editing.event_id;
    const kept = composer.kept?.filter((edit) => edit.message.event_id !== editedId);

    return { commands, composer: endEdit({ ...composer, kept }) };
  }

  const messages: Outgoing[] = [];
  const drafted = draftMessage(composer, account, key);

  if (drafted.type === 'problem') return { commands: [], composer, problem: drafted.problem };

  if (drafted.type === 'meet') {
    // The meeting opens in the draft's place; where none can start, the draft stays.
    if (!canStartMeeting) return { commands: [], composer, problem: { type: 'no_meetings' } };

    return {
      commands: [],
      composer: { ...composer, draft: '', pills: [], selection: undefined },
      after: 'meeting',
    };
  }

  if (drafted.type === 'command') {
    // The command acts in the draft's place; the reply and attachments wait for the next send.
    return {
      commands: [drafted.command],
      composer: { ...composer, draft: '', pills: [], selection: undefined },
      after: drafted.after,
    };
  }

  if (drafted.message) messages.push(drafted.message);

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
    composer: commands.length > 0 ? emptied(composer) : composer,
  };
}

/** How long the server may hold a scheduled message at most (MSC4140 caps it at 30 days). */
export const MAX_SCHEDULE_MS = 30 * 86_400_000;

/**
 * Why what a composer holds can't be scheduled: attachments (uploads go at once), a command
 * that acts rather than sends a message (`/leave` can't wait on the server), a command that
 * can't be done, or nothing to send (an edit, an empty draft).
 */
export type ScheduleProblem =
  | SendProblem
  | { type: 'attachments' }
  | { type: 'command' }
  | { type: 'nothing' };

/**
 * What stops a composer's message from being scheduled, or `null` when it may be: text alone,
 * or a command that makes a message (`/me`, `/plain`, `/spoiler`, `/shrug`, …).
 */
export function scheduleProblem(
  composer: ComposerState,
  account: AccountId,
  key: TimelineKey,
): ScheduleProblem | null {
  if (composer.editing || !hasText(composer)) return { type: 'nothing' };

  if ((composer.attachments?.length ?? 0) > 0) return { type: 'attachments' };
  const drafted = draftMessage(composer, account, key);

  switch (drafted.type) {
    case 'problem':
      return drafted.problem;
    case 'command':
    case 'meet':
      return { type: 'command' };
    case 'message':
      return drafted.message ? null : { type: 'nothing' };
  }
}

/**
 * The `Schedule` command for what a composer holds, sent in `delayMs`; `null` when it can't be
 * scheduled (`scheduleProblem`). The composer is emptied as a send empties it.
 */
export function planSchedule(
  composer: ComposerState,
  account: AccountId,
  key: TimelineKey,
  delayMs: number,
): SendPlan | null {
  if (scheduleProblem(composer, account, key)) return null;
  const drafted = draftMessage(composer, account, key);

  if (drafted.type !== 'message' || !drafted.message) return null;

  const command: Command = {
    type: 'Schedule',
    data: {
      account,
      key,
      reply_to: composer.replyTo?.event_id ?? null,
      message: drafted.message,
      delay_ms: Math.min(Math.max(0, Math.round(delayMs)), MAX_SCHEDULE_MS),
    },
  };

  return { commands: [command], composer: emptied(composer) };
}

function hasText(composer: ComposerState): boolean {
  return draftHasText(composer.draft);
}

/** Whether a composer has anything to send (or an edit to save). */
export function canSend(composer: ComposerState): boolean {
  if (composer.editing) return hasText(composer);

  return hasText(composer) || (composer.attachments?.length ?? 0) > 0;
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

/**
 * The words of a message a reply quotes: its text (each spoiler `[Spoiler]`, never its words),
 * its file's name, its poll's question.
 */
export function excerpt(message: Message): string {
  const content = message.content;

  switch (content.type) {
    case 'Notice':
    case 'Text':
      return [...spoilerSafeText(content.data.body, content.data.html)].slice(0, EXCERPT).join('');
    case 'Emote': {
      const words = spoilerSafeText(content.data.body, content.data.html);

      return [...`* ${message.sender_name} ${words}`].slice(0, EXCERPT).join('');
    }

    case 'Location':
      return content.data.description ?? coordinatesText(content.data.geo_uri) ?? '';
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

const IMAGE_EXTENSIONS = [
  'avif',
  'bmp',
  'gif',
  'heic',
  'heif',
  'jpeg',
  'jpg',
  'png',
  'svg',
  'webp',
];

const VIDEO_EXTENSIONS = ['3gp', 'm4v', 'mkv', 'mov', 'mp4', 'webm'];

/**
 * How a picked file previews in the composer: a picture or video by its media type, else by its
 * name's extension (pickers may give no type, or a generic one), else `fallback`.
 */
export function attachmentKind(
  mime: string | null | undefined,
  name: string,
  fallback: PendingAttachment['kind'] = 'file',
): PendingAttachment['kind'] {
  if (mime?.startsWith('image/')) return 'image';

  if (mime?.startsWith('video/')) return 'video';
  const extension = /\.([^./]+)$/.exec(name)?.[1]?.toLowerCase() ?? '';

  if (IMAGE_EXTENSIONS.includes(extension)) return 'image';

  if (VIDEO_EXTENSIONS.includes(extension)) return 'video';

  return fallback;
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
