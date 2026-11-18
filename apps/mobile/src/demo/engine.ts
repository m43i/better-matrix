// The demo engine: an engine with nothing behind it that answers like a homeserver would, from
// the fixtures. Every command finishes (so no screen waits forever); lists come back with the
// demo's members, threads, files and sessions; messages sent appear in their timeline; media the
// demo names is served from the app's own assets.

import { Engine, type EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  Command,
  Draft,
  Message,
  MessageContent,
  MessageHit,
  Outgoing,
  RoomList,
  RequestId,
  RoomPreview,
} from '@better-matrix/matrix-native/types';

import { messageText } from '../state/messages';
import { fuzzyMatch } from '../state/search';
import {
  DEMO_DAY_MS,
  DEMO_LINKED_ROOM,
  demoFiles,
  demoMembers,
  demoSessions,
  demoThreads,
  seedEvents,
  GENERAL,
  DEMO_THREAD,
  GRACE,
  KAI,
} from './fixtures';

/** Resolves a demo media ref to a plain file path, or `null` when the demo has no such media. */
export type MediaResolver = (media: string) => Promise<string | null>;

/** The events answering `command`, before its `Finished`; pure, so it can be tested. */
export function answer(
  command: Command,
  request: RequestId,
  now: number = DEMO_DAY_MS + 12 * 3_600_000,
): EngineEvent[] {
  switch (command.type) {
    case 'Discover':
      return [
        {
          type: 'HomeserverDiscovered',
          data: {
            request,
            homeserver: {
              server_name: command.data.server,
              url: `https://matrix.${command.data.server}/`,
              methods: { oauth: true, password: true },
            },
          },
        },
      ];
    case 'ListMembers':
      return [
        {
          type: 'Members',
          data: {
            request: null,
            account: command.data.account,
            room_id: command.data.room_id,
            members: demoMembers(),
          },
        },
      ];
    case 'ListPinnedMessages':
      return [
        {
          type: 'PinnedMessages',
          data: {
            account: command.data.account,
            room_id: command.data.room_id,
            messages:
              command.data.room_id === GENERAL
                ? [
                    {
                      event_id: DEMO_THREAD.root,
                      sender_name: 'Alan Turing',
                      sender_id: '@alan:example.org',
                      sender_avatar: null,
                      timestamp_ms: DEMO_DAY_MS + 11 * 3_600_000 + 15 * 60_000,
                      body: 'Cross-signing verification passes on Linux, macOS and Windows.',
                      thread: null,
                    },
                  ]
                : [],
          },
        },
      ];
    case 'ListThreads':
      return [
        {
          type: 'Threads',
          data: {
            account: command.data.account,
            room_id: command.data.room_id,
            threads: command.data.room_id === GENERAL ? demoThreads() : [],
          },
        },
      ];
    case 'ListFiles':
      return [
        {
          type: 'Files',
          data: {
            account: command.data.account,
            room_id: command.data.room_id,
            files: command.data.room_id === GENERAL ? demoFiles() : [],
          },
        },
      ];
    case 'ListSessions':
      return [{ type: 'Sessions', data: { account: command.data, sessions: demoSessions() } }];
    case 'SearchMessages': {
      const messages = new Map<string, { hit: MessageHit; rank: number }>();

      for (const event of seedEvents()) {
        if (event.type !== 'TimelineChanged' || event.data.account !== command.data.account)
          continue;

        for (const diff of event.data.diffs) {
          if (diff.type !== 'Reset') continue;

          for (const entry of diff.data) {
            if (entry.kind.type !== 'Message') continue;
            const message = entry.kind.data;

            if (message.content.type !== 'Text') continue;
            const body = message.content.data.body.replace(/\s+/g, ' ').trim();
            const match = fuzzyMatch(body, command.data.term);

            if (!match) continue;
            messages.set(message.event_id, {
              rank: match.rank,
              hit: {
                room_id: event.data.key.room_id,
                event_id: message.event_id,
                thread: event.data.key.thread,
                sender: message.sender_id,
                timestamp_ms: message.timestamp_ms,
                body,
                highlights: match.positions,
              },
            });
          }
        }
      }

      return [
        {
          type: 'MessagesFound',
          data: {
            request,
            messages: [...messages.values()]
              .sort((a, b) => a.rank - b.rank || b.hit.timestamp_ms - a.hit.timestamp_ms)
              .slice(0, 30)
              .map(({ hit }) => hit),
          },
        },
      ];
    }

    case 'SuggestPeople':
    case 'SearchPeople': {
      const term = command.type === 'SearchPeople' ? command.data.term.toLowerCase() : '';

      const people = demoMembers()
        .people.map((m) => m.person)
        .filter((p) => p.name.toLowerCase().includes(term) || p.id.includes(term));

      return [{ type: 'PeopleFound', data: { request, people } }];
    }

    case 'OpenDirectChat':
      return [
        {
          type: 'DirectChat',
          data: {
            request,
            room_id:
              command.data.user_id === GRACE ? '!dm-grace:example.org' : '!dm-kai:example.org',
          },
        },
      ];
    case 'PreviewRoom':
      return [{ type: 'RoomPreview', data: { request, preview: demoPreview(command.data.room) } }];
    case 'JoinRoom':
      return [
        {
          type: 'RoomJoined',
          data: { request, room_id: demoPreview(command.data.room_id).room_id },
        },
      ];
    case 'Send': {
      const content = sentContent(command.data.message);

      if (!content) return [];

      const message: Message = {
        event_id: `$sent-${request}`,
        sender_id: command.data.account,
        sender_name: 'Ada Lovelace',
        sender_avatar: null,
        content,
        edited: false,
        editable: content.type === 'Text',
        pending: false,
        timestamp_ms: now,
        reactions: [],
        reply_to: null,
        thread: null,
      };

      return [
        {
          type: 'TimelineChanged',
          data: {
            account: command.data.account,
            key: command.data.key,
            diffs: [
              {
                type: 'PushBack',
                data: { id: message.event_id, kind: { type: 'Message', data: message } },
              },
            ],
          },
        },
      ];
    }

    case 'StartCall':
    case 'AnswerCall':
      return [
        {
          type: 'CallUpdated',
          data: {
            call: 7,
            account: command.data.account,
            room_id: command.data.room_id,
            via: 'Rtc',
            phase: 'Connected',
            video: command.data.video,
            microphone: true,
            camera: null,
            screen: null,
            participants: [
              {
                key: 'kai',
                person: { id: KAI, name: 'Kai', avatar: null },
                muted: false,
                camera: null,
                screen: null,
              },
            ],
          },
        },
      ];
    case 'HangUp':
      return [];
    case 'SetStatus':
      return [{ type: 'Status', data: command.data }];
    default:
      return [];
  }
}

/** A room as a link shows it: #general (joined) by its alias or ID, else a public room nobody joined. */
function demoPreview(room: string): RoomPreview {
  if (room === GENERAL || room === '#general:example.org') {
    return {
      room_id: GENERAL,
      alias: '#general:example.org',
      name: 'general',
      topic: null,
      avatar: null,
      member_count: 6,
      join_rule: 'Invite',
      space: false,
      joined: true,
      invited: false,
    };
  }

  return { ...DEMO_LINKED_ROOM, room_id: room.startsWith('!') ? room : DEMO_LINKED_ROOM.room_id };
}

function sentContent(message: Outgoing): MessageContent | null {
  switch (message.type) {
    case 'Text':
      return { type: 'Text', data: message.data };
    case 'Poll':
      return {
        type: 'Poll',
        data: {
          question: message.data.question,
          answers: message.data.answers.map((text, i) => ({
            id: String(i),
            text,
            votes: 0,
            mine: false,
          })),
          disclosed: message.data.disclosed,
          max_selections: 1,
          voters: 0,
          ended: false,
        },
      };
    case 'File':
    case 'Voice': {
      const path = message.type === 'File' ? message.data.data : message.data.upload.data;
      const name = path.split('/').pop() ?? 'file';

      return {
        type: 'Attachment',
        data: {
          kind: message.type === 'Voice' ? 'Audio' : 'File',
          name,
          size: null,
          media: null,
          thumbnail: null,
          dimensions: null,
          audio: null,
        },
      };
    }
  }
}

/** A source message and its containing thread, from the shared demo fixtures. */
function demoMessage(account: string, roomId: string, eventId: string) {
  for (const event of seedEvents()) {
    if (
      event.type !== 'TimelineChanged' ||
      event.data.account !== account ||
      event.data.key.room_id !== roomId
    )
      continue;

    for (const diff of event.data.diffs) {
      if (diff.type !== 'Reset') continue;

      for (const entry of diff.data) {
        if (entry.kind.type === 'Message' && entry.kind.data.event_id === eventId)
          return { message: entry.kind.data, thread: event.data.key.thread };
      }
    }
  }

  return null;
}

/** Starts a demo engine. `resolveMedia` serves the demo's own media refs. */
export function demoEngine(resolveMedia: MediaResolver): Engine {
  let next = 1;

  const engine: Engine = new Engine({
    send(json) {
      const request = next++;
      // SAFETY: Engine.send serialized this Command from the shared generated engine types.
      const command = JSON.parse(json) as Command;
      // Answers come after the sender has its request ID, as from the real engine.
      setTimeout(() => void respond(command, request), 0);

      return request;
    },
  });

  const lists = new Map<string, RoomList>();
  /** Drafts kept as the engine's store keeps them, for the run of the demo. */
  const drafts = new Map<string, Draft>();
  engine.subscribe((event) => {
    if (event.type === 'RoomsUpdated') lists.set(event.data.account, event.data.list);
  });

  const finish = (request: RequestId, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  async function respond(command: Command, request: RequestId) {
    if (command.type === 'ListPinnedMessages') {
      const { account, room_id: roomId } = command.data;
      const pins = lists.get(account)?.rooms.find((r) => r.room_id === roomId)?.pinned ?? [];

      const messages = pins.map((eventId) => {
        const source = demoMessage(account, roomId, eventId);

        return {
          event_id: eventId,
          sender_name: source?.message.sender_name ?? '',
          sender_id: source?.message.sender_id ?? '',
          sender_avatar: source?.message.sender_avatar ?? null,
          timestamp_ms: source?.message.timestamp_ms ?? null,
          body: source ? messageText(source.message) : null,
          thread: source?.thread ?? null,
        };
      });

      engine.emit({ type: 'PinnedMessages', data: { account, room_id: roomId, messages } });
      finish(request);

      return;
    }

    if (command.type === 'SetPinned') {
      const { account, room_id: roomId, event_id: eventId, pinned } = command.data;
      const list = lists.get(account);

      if (list)
        engine.emit({
          type: 'RoomsUpdated',
          data: {
            account,
            list: {
              ...list,
              rooms: list.rooms.map((r) =>
                r.room_id === roomId
                  ? {
                      ...r,
                      pinned: pinned
                        ? [...new Set([...r.pinned, eventId])]
                        : r.pinned.filter((id) => id !== eventId),
                    }
                  : r,
              ),
            },
          },
        });
      finish(request);

      return;
    }

    if (command.type === 'ListSavedMessages') {
      const { account } = command.data;

      const messages = (lists.get(account)?.rooms ?? []).flatMap((room) =>
        room.saved.map((eventId) => {
          const source = demoMessage(account, room.room_id, eventId);

          return {
            room_id: room.room_id,
            saved_at_ms: source?.message.timestamp_ms ?? null,
            preview: {
              event_id: eventId,
              sender_name: source?.message.sender_name ?? '',
              sender_id: source?.message.sender_id ?? '',
              sender_avatar: source?.message.sender_avatar ?? null,
              timestamp_ms: source?.message.timestamp_ms ?? null,
              body: source ? messageText(source.message) : null,
              thread: source?.thread ?? null,
            },
          };
        }),
      );

      engine.emit({ type: 'SavedMessages', data: { account, messages } });
      finish(request);

      return;
    }

    if (command.type === 'SetSaved') {
      const { account, room_id: roomId, event_id: eventId, saved } = command.data;
      const list = lists.get(account);

      if (list)
        engine.emit({
          type: 'RoomsUpdated',
          data: {
            account,
            list: {
              ...list,
              rooms: list.rooms.map((r) =>
                r.room_id === roomId
                  ? {
                      ...r,
                      saved: saved
                        ? [eventId, ...r.saved.filter((id) => id !== eventId)]
                        : r.saved.filter((id) => id !== eventId),
                    }
                  : r,
              ),
            },
          },
        });
      finish(request);

      return;
    }

    if (command.type === 'SaveDraft') {
      const { account, key, draft } = command.data;
      const id = `${account}|${key.room_id}|${key.thread ?? ''}`;

      if (draft && draft.body.trim() !== '') drafts.set(id, draft);
      else drafts.delete(id);
      finish(request);

      return;
    }

    if (command.type === 'LoadDraft') {
      const { account, key } = command.data;
      const draft = drafts.get(`${account}|${key.room_id}|${key.thread ?? ''}`) ?? null;

      engine.emit({ type: 'DraftLoaded', data: { account, key, draft } });
      finish(request);

      return;
    }

    if (command.type === 'Forward') {
      const { account, key, event_id: eventId, room_id: roomId } = command.data;

      const source = demoMessage(account, key.room_id, eventId);

      if (!source) {
        finish(request, 'message unavailable');

        return;
      }

      const message = {
        ...source.message,
        event_id: `$forward-${request}`,
        sender_id: account,
        sender_name: 'Ada Lovelace',
        reply_to: null,
        thread: null,
        reactions: [],
        editable: source.message.content.type === 'Text',
      };

      engine.emit({
        type: 'TimelineChanged',
        data: {
          account,
          key: { room_id: roomId, thread: null },
          diffs: [
            {
              type: 'PushBack',
              data: { id: message.event_id, kind: { type: 'Message', data: message } },
            },
          ],
        },
      });
      finish(request);

      return;
    }

    if (command.type === 'FetchMedia') {
      const path = await resolveMedia(command.data.media).catch(() => null);

      if (path)
        engine.emit({
          type: 'MediaFile',
          data: { request, media: command.data.media, size: command.data.size, path },
        });
      finish(request, path ? undefined : 'not in the demo');

      return;
    }

    if (command.type === 'FetchGif') {
      finish(request, 'not in the demo');

      return;
    }

    for (const event of answer(command, request, Date.now())) engine.emit(event);
    finish(request);
  }

  return engine;
}
