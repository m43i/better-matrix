// Who has read a message, as the desktop shows it (`state/readers.rs`): the SDK's read positions,
// with each reader's name and picture from the room's members, else from a message of theirs.

import type {
  AccountId,
  Members,
  Person,
  ReadReceipt,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';

import { account as accountOf } from './selectors';
import { roomKey, timelineId } from './timeline';
import type { AccountData, State } from './types';

/** The most pictures one message's read indicator shows. */
export const READER_FACES = 4;

const NONE: ReadReceipt[] = [];

/**
 * Who has read up to `eventId` in a room's main timeline, newest first. Threads and messages
 * still being sent show none.
 */
export function readersOf(
  state: State,
  account: AccountId,
  roomId: string,
  eventId: string,
): ReadReceipt[] {
  const entries = accountOf(state, account)?.timelines[timelineId(roomKey(roomId))]?.entries;

  const found = entries?.find(
    (entry) => entry.kind.type === 'Message' && entry.kind.data.event_id === eventId,
  );

  if (found?.kind.type !== 'Message' || found.kind.data.pending) return NONE;

  return found.kind.data.read_receipts ?? NONE;
}

// Indexes built once per list: state is never changed in place, so a list's index stays true.
/** Members by user ID, per member list. */
const memberIndex = new WeakMap<Members, Map<string, Person>>();

/** Senders by user ID, per timeline's entries. */
const senderIndex = new WeakMap<TimelineEntry[], Map<string, Person>>();

function membersById(members: Members): Map<string, Person> {
  let index = memberIndex.get(members);

  if (!index) {
    index = new Map(members.people.map((member) => [member.person.id, member.person]));
    memberIndex.set(members, index);
  }

  return index;
}

function sendersById(entries: TimelineEntry[]): Map<string, Person> {
  let index = senderIndex.get(entries);

  if (!index) {
    index = new Map();

    // Oldest first, so each sender's newest name and picture win.
    for (const entry of entries) {
      if (entry.kind.type !== 'Message') continue;
      const message = entry.kind.data;
      index.set(message.sender_id, {
        id: message.sender_id,
        name: message.sender_name,
        avatar: message.sender_avatar,
      });
    }

    senderIndex.set(entries, index);
  }

  return index;
}

/** A reader as the room knows them: a member, else a sender in its timeline, else their ID. */
export function readerOf(data: AccountData | undefined, roomId: string, userId: string): Person {
  const members = data?.members[roomId];
  const member = members ? membersById(members).get(userId) : undefined;

  if (member) return member;
  const entries = data?.timelines[timelineId(roomKey(roomId))]?.entries;
  const sender = entries ? sendersById(entries).get(userId) : undefined;

  return sender ?? { id: userId, name: userId, avatar: null };
}
