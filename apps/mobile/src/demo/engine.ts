// The demo engine: an engine with nothing behind it that answers like a homeserver would, from
// the fixtures. Every command finishes (so no screen waits forever); lists come back with the
// demo's members, threads, files and sessions; messages sent appear in their timeline; media the
// demo names is served from the app's own assets.

import { Engine, type EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  Command,
  Message,
  MessageContent,
  Outgoing,
  RequestId,
  RoomPreview,
} from '@better-matrix/matrix-native/types';

import {
  DEMO_DAY_MS,
  DEMO_LINKED_ROOM,
  demoFiles,
  demoMembers,
  demoSessions,
  demoThreads,
  GENERAL,
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
        },
      };
    }
  }
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

  const finish = (request: RequestId, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  async function respond(command: Command, request: RequestId) {
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
