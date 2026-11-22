// Slash commands: a draft that starts with `/name` does what the command says instead of sending
// its text, as Element's and the desktop's composers do. Some send a message of another kind
// (`/me` an emote, `/shrug` text after a shrug, `/plain` text without formatting, `/spoiler`
// hidden text); the rest act on the room or account (`/invite`, `/join`, `/leave`, `/topic`,
// `/nick`). `//text` sends `/text`. Decided as data here, sent by `state/actions/composer.ts`.

import type { AccountId, Command, Outgoing } from '@better-matrix/matrix-native/types';

import type { Key } from '../i18n';
import { outgoingText } from './format';
import { type DraftPill, escapeHtml, mentionsOf, pillHtml, placePills, slicePills } from './pills';

/** One command the composer suggests: its name, what it takes, and what it does. */
export interface SlashCommandInfo {
  name: string;
  /** What follows the name, as the suggestion shows it (`<message>`); empty when nothing. */
  args: Key | null;
  about: Key;
}

/** Every command, in the order they're suggested. */
export const COMMANDS: readonly SlashCommandInfo[] = [
  { name: 'me', args: 'mobile.composer.command_args_action', about: 'mobile.composer.command_me' },
  {
    name: 'shrug',
    args: 'mobile.composer.command_args_message',
    about: 'mobile.composer.command_shrug',
  },
  {
    name: 'tableflip',
    args: 'mobile.composer.command_args_message',
    about: 'mobile.composer.command_tableflip',
  },
  {
    name: 'unflip',
    args: 'mobile.composer.command_args_message',
    about: 'mobile.composer.command_unflip',
  },
  {
    name: 'lenny',
    args: 'mobile.composer.command_args_message',
    about: 'mobile.composer.command_lenny',
  },
  {
    name: 'plain',
    args: 'mobile.composer.command_args_message',
    about: 'mobile.composer.command_plain',
  },
  {
    name: 'spoiler',
    args: 'mobile.composer.command_args_message',
    about: 'mobile.composer.command_spoiler',
  },
  {
    name: 'invite',
    args: 'mobile.composer.command_args_user',
    about: 'mobile.composer.command_invite',
  },
  {
    name: 'join',
    args: 'mobile.composer.command_args_room',
    about: 'mobile.composer.command_join',
  },
  { name: 'leave', args: null, about: 'mobile.composer.command_leave' },
  {
    name: 'topic',
    args: 'mobile.composer.command_args_topic',
    about: 'mobile.composer.command_topic',
  },
  {
    name: 'nick',
    args: 'mobile.composer.command_args_name',
    about: 'mobile.composer.command_nick',
  },
];

/** The faces the text commands put before the message. */
const FACES = new Map([
  ['shrug', '¯\\_(ツ)_/¯'],
  ['tableflip', '(╯°□°)╯︵ ┻━┻'],
  ['unflip', '┬──┬ ノ( ゜-゜ノ)'],
  ['lenny', '( ͡° ͜ʖ ͡°)'],
]);

/** The commands that start with what was typed after `/`. */
export function commandSuggestions(query: string): SlashCommandInfo[] {
  const lower = query.toLowerCase();

  return COMMANDS.filter((command) => command.name.startsWith(lower));
}

/** What a draft starting with a command does. */
export type Slash =
  /** Send this message in place of the draft's text. */
  | { type: 'message'; message: Outgoing }
  /** Send this command; `after` says where the app goes once it worked. */
  | { type: 'command'; command: Command; after: 'stay' | 'joined' | 'left' }
  /** No such command. */
  | { type: 'unknown'; name: string }
  /** The command needs something it wasn't given (`/invite` without a person). */
  | { type: 'usage'; name: string; args: Key };

/** Where a draft is written, for the commands that act on the room. */
export interface SlashContext {
  account: AccountId;
  roomId: string;
}

const COMMAND = /^\/([a-z]+)(?:[ \t]+([\s\S]*)|\n([\s\S]*))?$/i;

/** Markdown's markers in `text` escaped, so it is sent as written. */
function escapeMarkers(text: string): string {
  return text.replace(/[\\`*_~[\]()>#\-.!|$]/g, (c) => `\\${c}`);
}

/**
 * What the draft's command does; `null` when the draft isn't a command (it doesn't start with
 * `/` and a name), so it goes as text. `//text` sends `/text`.
 */
export function slashOf(
  draft: string,
  draftPills: readonly DraftPill[],
  context: SlashContext,
): Slash | null {
  const trimmed = draft.replace(/^\s+/, '');
  const lead = draft.length - trimmed.length;

  if (trimmed.startsWith('//'))
    return {
      type: 'message',
      message: {
        type: 'Text',
        data: outgoingText(trimmed.slice(1), slicePills(draftPills, lead + 1, draft.length)),
      },
    };
  const line = trimmed.replace(/\s+$/, '');
  const match = COMMAND.exec(line);

  if (!match) return null;
  const name = (match[1] ?? '').toLowerCase();
  const raw = match[2] ?? match[3] ?? '';
  const rest = raw.trim();
  // Where the arguments start in the draft: the pills in them are placed from there.
  const from = lead + line.length - raw.length + (raw.length - raw.trimStart().length);
  const pills = slicePills(draftPills, from, from + rest.length);
  const info = COMMANDS.find((command) => command.name === name);

  if (!info) return { type: 'unknown', name };
  const usage = (): Slash => ({ type: 'usage', name, args: info.args ?? info.about });
  const face = FACES.get(name);

  if (face !== undefined) {
    const before = escapeMarkers(face);
    const text = rest ? `${before} ${rest}` : before;
    const moved = slicePills(draftPills, from, from + rest.length, before.length + 1);

    return { type: 'message', message: { type: 'Text', data: outgoingText(text, moved) } };
  }

  switch (name) {
    case 'me':
      return rest
        ? { type: 'message', message: { type: 'Emote', data: outgoingText(rest, pills) } }
        : usage();
    case 'plain':
      return rest
        ? {
            type: 'message',
            message: {
              type: 'Text',
              data: { body: rest, html: null, mentions: mentionsOf(rest, pills) },
            },
          }
        : usage();
    case 'spoiler':
      return rest
        ? { type: 'message', message: { type: 'Text', data: spoilerText(rest, pills) } }
        : usage();

    case 'invite': {
      const userId = argumentId(rest, pills, 'user:', /^@\S+:\S+$/);

      return userId
        ? {
            type: 'command',
            command: { type: 'Invite', data: { ...room(context), user_id: userId } },
            after: 'stay',
          }
        : usage();
    }

    case 'join': {
      const roomId = argumentId(rest, pills, 'room:', /^[#!]\S+:\S+$/);

      return roomId
        ? {
            type: 'command',
            command: {
              type: 'JoinRoom',
              data: { account: context.account, room_id: roomId, via: [] },
            },
            after: 'joined',
          }
        : usage();
    }

    case 'leave':
      return {
        type: 'command',
        command: { type: 'LeaveRoom', data: { ...room(context), forget: false } },
        after: 'left',
      };
    case 'topic':
      return rest
        ? {
            type: 'command',
            command: { type: 'SetTopic', data: { ...room(context), topic: rest } },
            after: 'stay',
          }
        : usage();
    case 'nick':
      return rest
        ? {
            type: 'command',
            command: { type: 'SetDisplayName', data: { account: context.account, name: rest } },
            after: 'stay',
          }
        : usage();
    default:
      return { type: 'unknown', name };
  }
}

/**
 * `text` sent hidden, written as it stands: Markdown isn't read (so nothing in it can close the
 * spoiler early), its pills go as links, line breaks as `<br />`, and the plain body says only
 * `[Spoiler]`, as a formatted spoiler's does.
 */
export function spoilerText(text: string, pills: readonly DraftPill[]) {
  let html = '';
  let at = 0;

  for (const placed of placePills(text, pills)) {
    html += escapeHtml(text.slice(at, placed.start)) + pillHtml(placed.pill);
    at = placed.end;
  }

  html += escapeHtml(text.slice(at));

  return {
    body: '[Spoiler]',
    html: `<p><span data-mx-spoiler>${html.replace(/\r?\n/g, '<br />')}</span></p>`,
    mentions: mentionsOf(text, pills),
  };
}

function room(context: SlashContext) {
  return { account: context.account, room_id: context.roomId };
}

/**
 * The ID a command's argument names: the pill it is (a mention picked from the suggestions), or
 * the ID typed out.
 */
function argumentId(
  text: string,
  pills: readonly DraftPill[],
  prefix: string,
  typed: RegExp,
): string | null {
  const placed = placePills(text, pills).find((p) => p.pill.id.startsWith(prefix));

  if (placed && text.slice(0, placed.start).trim() === '')
    return placed.pill.id.slice(prefix.length);
  const word = text.split(/\s+/)[0] ?? '';

  return typed.test(word) ? word : null;
}
