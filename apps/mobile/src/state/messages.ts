// What a message shows, worked out once per message (the desktop's `TimelineItem`): its text as
// blocks, a linked GIF and the link that gets a card; and the rules its parts follow: polls'
// votes, call cards' status, image preview sizes, which actions the user has on it.

import { z } from 'zod';

import { type EditDraft, editDraft } from './format';
import { matches } from './people';
import type {
  AccountId,
  Attachment,
  CallCard,
  CallVia,
  MediaRef,
  MediaSize,
  Message,
  Poll,
  RoomSummary,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';

import { t } from '../i18n';
import { type Block, blocksText, hasSpoiler, htmlBlocks, type Inline, plainBlocks } from './html';
import { findGifLink, findPreviewLink, htmlWithoutLink, withoutLink } from './links';
import { Lru } from './lru';

/** A text message ready to draw. */
export interface Prepared {
  /** The text as blocks; `null` for messages that aren't text. */
  blocks: Block[] | null;
  /** A GIF linked from the text, shown below the rest of it (`rest`, if any) in place of the link. */
  gif: { url: string; rest: Block[] | null } | null;
  /** The first other link, which gets a preview card. */
  link: string | null;
}

const prepared = new WeakMap<Message, Prepared>();

/** What a message that isn't text shows: none of it. */
const NOT_TEXT: Prepared = { blocks: null, gif: null, link: null };

/** Recent texts' parses, by `textKey`: shared by the messages that say the same. */
const parsed = new Lru<string, Prepared>(500);

/** One key per body and HTML: the body's length marks where the HTML begins. */
function textKey(body: string, html: string | null): string {
  return html === null ? `p${body}` : `h${body.length}:${body}${html}`;
}

function parseText(body: string, html: string | null): Prepared {
  const blocksOf = (plain: string, markup: string | null) =>
    markup ? htmlBlocks(markup) : plainBlocks(plain);

  // Links are read from the plain body, which may name what a spoiler hides: a message holding
  // one fetches no preview and plays no GIF, so nothing of it shows before it is tapped.
  if (html && hasSpoiler(html)) return { blocks: blocksOf(body, html), gif: null, link: null };

  const gifUrl = findGifLink(body);
  let gif: Prepared['gif'] = null;

  if (gifUrl) {
    const plain = withoutLink(body, gifUrl);
    const markup = html ? htmlWithoutLink(html, gifUrl) : null;
    gif = { url: gifUrl, rest: plain === '' ? null : blocksOf(plain, markup) };
  }

  return { blocks: blocksOf(body, html), gif, link: findPreviewLink(body) };
}

/**
 * `message` ready to draw, parsed once per message (messages are replaced, never changed) and
 * once per text: a message replaced for a receipt or a reaction keeps the same parse, so the
 * views drawing its blocks needn't redraw.
 */
export function prepare(message: Message, sender = message.sender_name): Prepared {
  const canMemo = message.content.type !== 'Emote' || sender === message.sender_name;
  const known = canMemo ? prepared.get(message) : undefined;

  if (known) return known;
  let result = NOT_TEXT;

  if (message.content.type === 'Text' || message.content.type === 'Notice') {
    const { body, html } = message.content.data;
    const key = textKey(body, html);
    const kept = parsed.get(key);

    if (kept) {
      result = kept;
    } else {
      result = parseText(body, html);
      parsed.set(key, result);
    }
  } else if (message.content.type === 'Emote') {
    // An emote reads as "* Ada waves": the sender's name before the action.
    const { body, html } = message.content.data;
    const key = `e${sender.length}:${sender}${textKey(body, html)}`;
    const kept = parsed.get(key);

    if (kept) {
      result = kept;
    } else {
      const text = parseText(body, html);
      result = {
        blocks: emoteBlocks(sender, text.blocks ?? []),
        gif: null,
        link: text.link,
      };
      parsed.set(key, result);
    }
  }

  if (canMemo) prepared.set(message, result);

  return result;
}

/**
 * The media handle of an unencrypted `mxc://` image a message names in its HTML (a custom
 * emoji). Handles are the engine's (`MediaRef::of(MediaSource::Plain)`, its JSON), which the
 * engine hands over for everything else; HTML carries bare `mxc://` URIs, so this one is written
 * here the same way.
 */
export function mxcMedia(url: string): MediaRef {
  return JSON.stringify({ url });
}

/**
 * Whether media is an encrypted file (`MediaSource::Encrypted`, written `{ file }`, where plain
 * media is `{ url }`): copying it into a pack uploads it unencrypted, so the app asks first.
 */
export function isEncryptedMedia(media: MediaRef): boolean {
  try {
    return ENCRYPTED_SOURCE.safeParse(JSON.parse(media)).success;
  } catch {
    return false;
  }
}

/** An encrypted `MediaSource`, as the engine writes it. */
const ENCRYPTED_SOURCE = z.object({ file: z.object({}).loose() });

/** An emote's blocks: "* Name" before the first paragraph's words. */
function emoteBlocks(sender: string, blocks: Block[]): Block[] {
  const prefix: Inline = { type: 'text', text: `* ${sender} `, marks: { italic: true } };
  const [first, ...rest] = blocks;

  if (first?.type === 'paragraph')
    return [{ ...first, inlines: [prefix, ...first.inlines] }, ...rest];

  return [{ type: 'paragraph', inlines: [prefix] }, ...blocks];
}

/** The link that gets a card: the first that isn't a GIF, else the GIF's while GIFs don't play. */
export function cardLink(item: Prepared, gifsShown: boolean): string | null {
  return item.link ?? (item.gif && !gifsShown ? item.gif.url : null);
}

/**
 * The message's text, for copying and translating: its words as they read, each spoiler
 * `[Spoiler]` (what it hides never leaves the message), or a file's name.
 */
export function messageText(message: Message): string | null {
  switch (message.content.type) {
    case 'Notice':
    case 'Text': {
      const { body, html } = message.content.data;

      return html ? blocksText(htmlBlocks(html), 'hidden') : body;
    }

    case 'Emote': {
      const { body, html } = message.content.data;

      return `* ${message.sender_name} ${html ? blocksText(htmlBlocks(html), 'hidden') : body}`;
    }

    case 'Location':
      return message.content.data.description ?? message.content.data.geo_uri;

    case 'Attachment':
      return message.content.data.name;
    case 'Poll':
      return message.content.data.question;
    default:
      return null;
  }
}

/** The plain text an edit starts from: what the user typed. */
export function editText(message: Message): string {
  return editDraftOf(message).draft;
}

/**
 * The draft an edit starts from, with its pills: formatted messages come back as the composer's
 * markers and their mentions and custom emoji as pills, so an edit keeps them.
 */
export function editDraftOf(message: Message): EditDraft {
  return message.content.type === 'Text'
    ? editDraft(message.content.data.body, message.content.data.html)
    : { draft: '', pills: [] };
}

/** A local echo, not yet on the server: nothing can act on it by event ID. */
export function isLocal(message: Message): boolean {
  return message.pending || message.event_id.startsWith('local:');
}

/** What the user may do with a message, as the desktop's toolbar and menu offer it. */
export interface MessageActions {
  forward: boolean;
  link: boolean;
  pin: boolean;
  /** Saving it for the user alone (or no longer). */
  save: boolean;
  translate: boolean;
  react: boolean;
  reply: boolean;
  thread: boolean;
  copy: boolean;
  edit: boolean;
  delete: boolean;
  endPoll: boolean;
  /** The attachment, to save or share. */
  file: boolean;
  /** Reporting someone else's message to the homeserver's admins. */
  report: boolean;
  /** Adding its sticker or picture to the user's own stickers. */
  saveSticker: boolean;
}

export function messageActions(
  message: Message,
  account: AccountId,
  inThread: boolean,
  canPin = false,
): MessageActions {
  const own = message.sender_id === account;
  const content = message.content;
  const sent = !isLocal(message);

  const readable =
    content.type !== 'Call' && content.type !== 'Redacted' && content.type !== 'Undecryptable';

  const text = messageText(message);
  const hasText = text !== null && text.trim() !== '';

  return {
    forward:
      sent &&
      (content.type === 'Text' || content.type === 'Notice' || content.type === 'Attachment'),
    link: sent && content.type !== 'Call',
    pin: sent && readable && canPin,
    save: sent && readable,
    translate: (content.type === 'Text' || content.type === 'Notice') && hasText,
    // A call's card takes no reactions: the timeline keeps none on it.
    react: sent && readable,
    reply: sent && readable,
    thread: sent && readable && !inThread,
    copy:
      (content.type === 'Text' ||
        content.type === 'Notice' ||
        content.type === 'Emote' ||
        content.type === 'Location' ||
        content.type === 'Poll') &&
      hasText,
    edit: sent && own && message.editable,
    delete: sent && own && content.type !== 'Redacted',
    endPoll: sent && own && content.type === 'Poll' && !content.data.ended,
    file: content.type === 'Attachment' && content.data.media !== null,
    report: sent && !own && content.type !== 'Redacted',
    saveSticker:
      sent &&
      content.type === 'Attachment' &&
      content.data.kind === 'Image' &&
      content.data.media !== null,
  };
}

/** Forwarding destinations follow the sidebar: rooms first, then direct messages. */
export function forwardSections(rooms: readonly RoomSummary[], query: string) {
  return (['Group', 'Direct'] as const).flatMap((kind) => {
    const data = rooms.filter((room) => room.kind === kind && matches(room.name, query));

    return data.length > 0 ? [{ kind, data }] : [];
  });
}

/**
 * Whether a message keeps the warm mark of one that mentions the user: push rules highlighted
 * it (`Message.highlighted`) and someone else sent it.
 */
export function marksMention(message: Message, account: AccountId): boolean {
  return message.highlighted && message.sender_id !== account;
}

/** The message with this event ID among `entries`. */
export function findMessage(
  entries: readonly TimelineEntry[],
  eventId: string,
): Message | undefined {
  for (const entry of entries) {
    if (entry.kind.type === 'Message' && entry.kind.data.event_id === eventId)
      return entry.kind.data;
  }

  return undefined;
}

// ----- attachments -------------------------------------------------------------------------

export type AttachmentAction = 'copyImage' | 'open' | 'share' | 'save';

/**
 * File actions shared by a message's long-press sheet and its attachment preview. None while the
 * account keeps the media back (`held`, ADR 0011): opening, sharing or saving would fetch it.
 */
export function attachmentActions(attachment: Attachment, held = false): AttachmentAction[] {
  if (!attachment.media || held) return [];

  return attachment.kind === 'Image'
    ? ['copyImage', 'open', 'share', 'save']
    : ['open', 'share', 'save'];
}

/** A row of a file action sheet: an action, or Show while its media is held back. */
export type AttachmentRow = AttachmentAction | 'show';

/**
 * What a file action sheet offers: its actions, or, where the sheet stands for the media alone
 * (`offersShow`), Show by itself while the media is held back, rather than nothing (ADR 0011).
 */
export function attachmentRows(
  attachment: Attachment,
  held: boolean,
  offersShow: boolean,
): AttachmentRow[] {
  if (held && offersShow && attachment.media) return ['show'];

  return attachmentActions(attachment, held);
}

/** The size timelines fetch image previews at, as the desktop's `MediaSize::PREVIEW`. */
const PREVIEW: MediaSize = { type: 'Thumbnail', data: { width: 800, height: 600 } };

/** The media to fetch for an image's preview and the size to ask for, as `Attachment::preview`. */
export function attachmentPreview(
  attachment: Attachment,
): { media: MediaRef; size: MediaSize } | null {
  if (attachment.kind !== 'Image') return null;

  if (attachment.thumbnail) return { media: attachment.thumbnail, size: { type: 'Original' } };

  // A sticker is small and may move: its file as sent, which a server thumbnail would freeze.
  if (attachment.media && attachment.sticker)
    return { media: attachment.media, size: { type: 'Original' } };

  if (attachment.media) return { media: attachment.media, size: PREVIEW };

  return null;
}

/** Fits `dimensions` into a `box` without upscaling; a 4:3 box when unknown. */
export function previewSize(
  dimensions: readonly [number, number] | null,
  box: { width: number; height: number },
) {
  const [w, h] = dimensions ?? [0, 0];

  if (w <= 0 || h <= 0) return { width: box.width, height: Math.round(box.height * 0.75) };
  const scale = Math.min(box.width / w, box.height / h, 1);

  return { width: Math.round(w * scale), height: Math.round(h * scale) };
}

/** `m:ss` of seconds. */
export function playTime(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));

  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

// ----- polls -------------------------------------------------------------------------------

/**
 * The user's vote after pressing `answer`: in a single-choice poll that answer, or none when it
 * was theirs (taking the vote back); in a multiple-choice one their picks with it toggled, up to
 * the poll's limit.
 */
export function nextVote(poll: Poll, answer: string): string[] {
  const mine = poll.answers.filter((a) => a.mine).map((a) => a.id);

  if (mine.includes(answer)) return mine.filter((id) => id !== answer);

  if (poll.max_selections <= 1) return [answer];

  return mine.length < poll.max_selections ? [...mine, answer] : mine;
}

/** Whether a poll's counts show: as people vote when disclosed, else once it ended. */
export function pollShowsResults(poll: Poll): boolean {
  return poll.disclosed || poll.ended;
}

/** The answers that won an ended poll: the most votes, if any. */
export function pollWinners(poll: Poll): Set<string> {
  if (!poll.ended) return new Set();
  const top = Math.max(0, ...poll.answers.map((a) => a.votes));

  return new Set(top > 0 ? poll.answers.filter((a) => a.votes === top).map((a) => a.id) : []);
}

/** The line under a poll: final results, the votes so far, or that they're hidden. */
export function pollStatus(poll: Poll): string {
  if (poll.ended) return t('poll.final', { count: poll.voters });

  if (poll.disclosed) return t('poll.votes', { count: poll.voters });

  return t('poll.hidden', { count: poll.voters });
}

// ----- calls -------------------------------------------------------------------------------

export interface CallStatus {
  /** The call is under way now (only the newest card can say so). */
  ongoing: boolean;
  /** What the card says under its title. */
  label: string;
  /** Offer to join: a room call under way that the user isn't in, where the room allows calls. */
  join: boolean;
  /** The user is in it here. */
  joined: boolean;
}

/** How a call's card stands, as the desktop's `call_card` decides. */
export function callStatus(
  card: CallCard,
  room: Pick<RoomSummary, 'call' | 'kind'>,
  options: { latest: boolean; callHere: CallVia | null; ringing: boolean },
): CallStatus {
  const people = room.call.participants;

  const ongoing =
    options.latest && (card.direct ? options.callHere === 'Direct' : people.length > 0);

  const declined = card.declined_by.length > 0 && room.kind === 'Direct';
  let label: string;

  if (ongoing && card.direct) label = t('call.card.ongoing');
  else if (ongoing)
    label = `${t('call.card.ongoing')} · ${t('call.people', { count: people.length })}`;
  else if (options.latest && card.direct && options.ringing) label = t('call.card.ringing');
  else if (declined) label = t('call.card.declined');
  else label = t('call.card.ended');
  const joined = ongoing && options.callHere !== null;

  return {
    ongoing,
    label,
    joined,
    join: ongoing && !joined && !card.direct && room.call.via !== null,
  };
}
