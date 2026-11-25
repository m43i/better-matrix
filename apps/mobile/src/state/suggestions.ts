// What the composer suggests for the word being typed (`pills.ts`'s `triggerAt`), as Slack's
// mobile composer lists them above the field: people in the room for `@` (and `@room` in a
// group), joined rooms for `#`, custom and standard emoji for `:code`, commands for `/` at the
// start. Pure: derived from the state, chosen by the composer.

import type { AccountId, ImagePack, MediaRef, PackImage } from '@better-matrix/matrix-native/types';

import { searchEmoji } from './emoji';
import { matches } from './people';
import { EVERYONE, emotePill, type Pill, roomPill, type Trigger, userPill } from './pills';
import { commandSuggestions, type SlashCommandInfo } from './slash';
import { account as accountOf } from './selectors';
import type { State } from './types';

/** How many suggestions show at most. */
export const SUGGESTIONS = 8;

export type Suggestion =
  /** A person, a room or `@room`: inserted as a pill. */
  | {
      type: 'pill';
      key: string;
      pill: Pill;
      /** What shows under the label (a user ID), if anything. */
      detail: string | null;
      /** Whose face to draw: a person's or a room's, by ID; none for `@room`. */
      face: { id: string; name: string; media: MediaRef | null } | null;
    }
  /** A custom emoji from an image pack: inserted as a pill. */
  | { type: 'emote'; key: string; pill: Pill; image: PackImage }
  /** A standard emoji: inserted as itself. */
  | { type: 'emoji'; key: string; emoji: string; shortcode: string }
  /** A command: completes the command's name. */
  | { type: 'command'; key: string; command: SlashCommandInfo };

/** The images of `packs` usable as `use`, each short code once (the user's own pack first). */
export function packImages(
  packs: readonly ImagePack[],
  use: 'emoticon' | 'sticker',
): Array<{ pack: ImagePack; image: PackImage }> {
  const seen = new Set<string>();
  const out: Array<{ pack: ImagePack; image: PackImage }> = [];

  for (const pack of packs) {
    for (const image of pack.images) {
      if (!image[use] || seen.has(`${image.shortcode}|${image.url}`)) continue;
      seen.add(`${image.shortcode}|${image.url}`);
      out.push({ pack, image });
    }
  }

  return out;
}

/** The packs usable in a room: listed for the room, else the account's own. */
export function roomPacks(state: State, account: AccountId, roomId: string): ImagePack[] {
  const data = accountOf(state, account);

  return data?.packs[roomId] ?? data?.packs[''] ?? [];
}

/** What to suggest for `trigger` in a room. */
export function suggestionsFor(
  state: State,
  account: AccountId,
  roomId: string,
  trigger: Trigger,
): Suggestion[] {
  const data = accountOf(state, account);

  if (!data) return [];
  const query = trigger.query;

  switch (trigger.kind) {
    case 'user': {
      const people = (data.members[roomId]?.people ?? [])
        .map((member) => member.person)
        .filter(
          (person) =>
            person.id !== account &&
            (matches(person.name, query) || person.id.slice(1).startsWith(query.toLowerCase())),
        )
        .slice(0, SUGGESTIONS)
        .map((person): Suggestion => ({
          type: 'pill',
          key: `user:${person.id}`,
          pill: userPill(person.id, person.name),
          detail: person.id,
          face: { id: person.id, name: person.name, media: person.avatar },
        }));

      const group = data.rooms.find((r) => r.room_id === roomId)?.kind !== 'Direct';
      const everyone = group && 'room'.startsWith(query.toLowerCase());

      return everyone
        ? [
            ...people.slice(0, SUGGESTIONS - 1),
            { type: 'pill', key: 'everyone', pill: EVERYONE, detail: null, face: null },
          ]
        : people;
    }

    case 'room':
      return data.rooms
        .filter((room) => room.kind === 'Group' && matches(room.name, query))
        .slice(0, SUGGESTIONS)
        .map((room) => ({
          type: 'pill',
          key: `room:${room.room_id}`,
          pill: roomPill(room.room_id, room.name),
          detail: null,
          face: { id: room.room_id, name: room.name, media: room.avatar },
        }));
    case 'emoji': {
      const lower = query.toLowerCase();

      const custom = packImages(roomPacks(state, account, roomId), 'emoticon')
        .filter(({ image }) => image.shortcode.toLowerCase().includes(lower))
        .map(({ image }): Suggestion => ({
          type: 'emote',
          key: `emote:${image.shortcode}:${image.url}`,
          pill: emotePill(image.url, image.shortcode),
          image,
        }));

      const standard = searchEmoji(query).map((emoji): Suggestion => ({
        type: 'emoji',
        key: `emoji:${emoji.emoji}`,
        emoji: emoji.emoji,
        shortcode: emoji.shortcodes[0] ?? emoji.label,
      }));

      return [...custom, ...standard].slice(0, SUGGESTIONS);
    }

    case 'command': {
      const canStartMeeting =
        data.rooms.find((r) => r.room_id === roomId)?.can_start_meeting ?? false;

      return commandSuggestions(query, canStartMeeting).map((command) => ({
        type: 'command',
        key: `command:${command.name}`,
        command,
      }));
    }
  }
}
