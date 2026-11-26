// The demo engine: an engine with nothing behind it that answers like a homeserver would, from
// the fixtures. Every command finishes (so no screen waits forever); lists come back with the
// demo's members, threads, files and sessions; messages sent appear in their timeline; media the
// demo names is served from the app's own assets. Image packs (and edits to them), GIFs found,
// places, edit histories, where a message is and scheduled messages are answered from the
// fixtures too.

import { Engine, type EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  Answer,
  Command,
  Draft,
  JoinRule,
  Message,
  MessageContent,
  MessageHit,
  Outgoing,
  Members,
  RoomList,
  RequestId,
  RoomPreview,
  ImagePack,
  ScheduledMessage,
  Safety,
  RoomSettings,
  RoomSummary,
} from '@better-matrix/matrix-native/types';

import { demoLiveLocations } from './liveLocations';
import { withPending } from '../state/admin';
import { mapOf } from '../state/geo';
import { messageText } from '../state/messages';
import { demoGifPage, DEMO_GIFS, editedPacks, editedPlace, packsFor } from './stickers';
import { DEFAULT_SAFETY, withChange } from '../state/safety';
import { roleLevel } from '../state/people';
import { fuzzyMatch } from '../state/search';
import {
  DEMO_DAY_MS,
  DEMO_LINKED_ROOM,
  DEMO_QR_BYTES,
  demoIdentity,
  PASSWORD_SERVER,
  demoQr,
  demoFiles,
  demoMembers,
  demoRoomSettings,
  demoSessions,
  demoSource,
  demoThreads,
  demoWidgetPermissions,
  demoWidgets,
  seedEvents,
  GENERAL,
  DEMO_THREAD,
  demoPacks,
  demoPlaces,
  GRACE,
  KAI,
  SIGN_OFF,
  signOffVersions,
  verificationEmojis,
  LAB_V2,
  NOTES,
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
    case 'Discover': {
      // Servers sign in with OAuth (and a QR code) and passwords, except the one that only takes
      // passwords, which creates accounts in the app.
      const passwordOnly = command.data.server === PASSWORD_SERVER;

      return [
        {
          type: 'HomeserverDiscovered',
          data: {
            request,
            homeserver: {
              server_name: command.data.server,
              url: `https://matrix.${command.data.server}/`,
              methods: {
                oauth: !passwordOnly,
                password: true,
                create_account: true,
                qr: !passwordOnly,
              },
            },
          },
        },
      ];
    }

    case 'ListMembers':
      return [
        {
          type: 'Members',
          data: {
            request: null,
            account: command.data.account,
            room_id: command.data.room_id,
            members: demoMembers(command.data.room_id),
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
    case 'ListWidgets':
      return [roomWidgets(command.data.account, command.data.room_id)];
    // A room's widgets come with its timeline, as the engine sends them.
    case 'OpenTimeline':
      return command.data.key.thread === null
        ? [roomWidgets(command.data.account, command.data.key.room_id)]
        : [];
    case 'ViewSource':
      return [
        {
          type: 'EventSource',
          data: { request, source: demoSource(command.data.room_id, command.data.event_id) },
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
    case 'OpenNoteToSelf':
      return [{ type: 'DirectChat', data: { request, room_id: NOTES } }];
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
        highlighted: false,
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
    case 'ListImagePacks':
      return [
        {
          type: 'ImagePacks',
          data: {
            account: command.data.account,
            room_id: command.data.room_id,
            packs: packsFor(demoPacks(), command.data.room_id),
            may_add: true,
          },
        },
      ];
    case 'Locate':
      return [
        {
          type: 'PositionFound',
          data: { request, estimated: true, geo_uri: 'geo:52.516275,13.377704' },
        },
      ];
    case 'SearchPlaces':
      return [{ type: 'PlacesFound', data: { request, places: demoPlaces(command.data.query) } }];
    case 'ListEdits': {
      const { account, room_id: roomId, event_id: eventId } = command.data;

      return [
        {
          type: 'EditHistory',
          data: {
            account,
            room_id: roomId,
            event_id: eventId,
            versions: eventId === SIGN_OFF ? signOffVersions() : [],
          },
        },
      ];
    }

    case 'LocateMessage': {
      const { account, room_id: roomId, event_id: eventId } = command.data;
      const thread = demoMessage(account, roomId, eventId)?.thread ?? null;

      return [
        {
          type: 'MessageLocated',
          data: { request, account, key: { room_id: roomId, thread }, event_id: eventId },
        },
      ];
    }

    // As the engine: the flow's ID answers the request, before any of its steps.
    case 'VerifyThisSession':
    case 'VerifyUser': {
      const account = command.type === 'VerifyUser' ? command.data.account : command.data;
      const flowId = command.type === 'VerifyUser' ? `demo-${command.data.user_id}` : 'demo-self';

      return [
        { type: 'VerificationRequested', data: { request, account, flow_id: flowId } },
        {
          type: 'Verification',
          data: {
            account,
            flow_id: flowId,
            step: { type: 'Ready', data: { qr: demoQr(), scan: true } },
          },
        },
      ];
    }

    case 'AcceptVerification':
      return [
        {
          type: 'Verification',
          data: { ...command.data, step: { type: 'Ready', data: { qr: demoQr(), scan: true } } },
        },
      ];
    case 'CompareEmoji':
      return [
        {
          type: 'Verification',
          data: { ...command.data, step: { type: 'Compare', data: verificationEmojis() } },
        },
      ];
    case 'ScanVerificationCode':
      return [
        {
          type: 'Verification',
          data: {
            account: command.data.account,
            flow_id: command.data.flow_id,
            step: { type: 'Done' },
          },
        },
      ];
    case 'ConfirmVerification':
      return [
        {
          type: 'Verification',
          data: {
            account: command.data.account,
            flow_id: command.data.flow_id,
            step: command.data.matches
              ? { type: 'Done' }
              : { type: 'Cancelled', data: 'they don’t match' },
          },
        },
      ];
    case 'LoadIdentity':
      return [
        {
          type: 'Identity',
          data: { ...command.data, identity: demoIdentity(command.data.user_id) },
        },
      ];
    case 'ReadQrCode':
      return [{ type: 'QrCodeRead', data: { request, code: DEMO_QR_BYTES } }];
    case 'ImportRoomKeys':
      return [{ type: 'KeysImported', data: { request, import: { imported: 112, total: 134 } } }];
    case 'SetOfflineDevice':
      return [
        {
          type: 'SecurityChanged',
          data: {
            account: command.data.account,
            security: {
              session_verified: true,
              recovery: 'Enabled',
              offline_device: command.data.on ? 'On' : 'Off',
            },
          },
        },
      ];
    default:
      return [];
  }
}

/** A room's widgets as the engine reports them: #general's three, none elsewhere. */
function roomWidgets(account: string, roomId: string): EngineEvent {
  return {
    type: 'Widgets',
    data: {
      account,
      room_id: roomId,
      widgets: roomId === GENERAL ? demoWidgets() : { widgets: [], can_add: true },
    },
  };
}

/** The harmless page every demo widget shows: the demo has no widget servers. */
export const DEMO_WIDGET_PAGE = 'https://example.org/';

/** A widget run's start in the demo: what it reports, and whether it then waits. */
export interface WidgetRunStart {
  events: EngineEvent[];
  /** Runs until answered to the end or cancelled; else it finishes, with `error` if given. */
  waits: boolean;
  error?: string;
}

/**
 * How the demo starts a widget run (`RunWidget`, `StartMeeting`, `OpenIntegrations`); `null` for
 * any other command. In the browser it opens the widget's address; in the app it shows the
 * demo's page: a meeting as itself, another widget over the widget API asking for permissions
 * (`demoWidgetPermissions`), the integration manager after its terms.
 */
export function widgetRun(command: Command, request: RequestId): WidgetRunStart | null {
  switch (command.type) {
    case 'RunWidget': {
      const widget = demoWidgets().widgets.find((w) => w.id === command.data.widget_id);

      if (command.data.room_id !== GENERAL || !widget)
        return { events: [], waits: false, error: 'no such widget' };

      if (command.data.browser)
        return { events: [{ type: 'OpenUrl', data: { request, url: widget.url } }], waits: false };
      const bridge = widget.meeting === null ? 'WidgetApi' : 'None';
      const page = { url: DEMO_WIDGET_PAGE, title: widget.name, bridge } as const;
      const events: EngineEvent[] = [{ type: 'WidgetPage', data: { request, page } }];

      if (widget.meeting === null)
        events.push({
          type: 'WidgetPermissions',
          data: { request, question: 1, widget: widget.name, permissions: demoWidgetPermissions() },
        });

      return { events, waits: true };
    }

    case 'StartMeeting':
      if (command.data.browser)
        return {
          events: [{ type: 'OpenUrl', data: { request, url: DEMO_WIDGET_PAGE } }],
          waits: false,
        };

      return {
        events: [
          {
            type: 'WidgetPage',
            data: { request, page: { url: DEMO_WIDGET_PAGE, title: 'Jitsi', bridge: 'None' } },
          },
        ],
        waits: true,
      };
    case 'OpenIntegrations':
      return {
        events: [{ type: 'IntegrationTerms', data: { request, policies: POLICIES } }],
        waits: true,
      };
    default:
      return null;
  }
}

/** What a demo widget run reports after `answer`: the integration manager once agreed to. */
export function widgetAnswered(answer: Answer, request: RequestId): EngineEvent[] {
  if (answer.type !== 'Agree') return [];
  const page = { url: DEMO_WIDGET_PAGE, title: 'Integrations', bridge: 'Integrations' } as const;

  return [{ type: 'WidgetPage', data: { request, page } }];
}

/**
 * The steps a long-running sign-in command reports first, in the demo, before it waits for an
 * answer (or a cancel): what its screen shows at the start.
 */
export function asking(command: Command, request: RequestId): EngineEvent[] {
  switch (command.type) {
    case 'LinkDevice':
      return [
        {
          type: 'QrLogin',
          data: {
            request,
            step:
              command.data.scanned === null
                ? { type: 'Show', data: demoQr() }
                : { type: 'CheckCode', data: 7 },
          },
        },
      ];
    case 'SignInWithQr':
      return [
        {
          type: 'QrLogin',
          data: {
            request,
            step:
              command.data.type === 'Show'
                ? { type: 'Show', data: demoQr() }
                : { type: 'CheckCode', data: 7 },
          },
        },
      ];
    case 'Register':
      return [{ type: 'Registration', data: { request, step: { type: 'Terms', data: POLICIES } } }];
    default:
      return [];
  }
}

/** What a demo registration asks next, after `answer`. */
export function nextAsked(answer: Answer, request: RequestId): EngineEvent[] {
  switch (answer.type) {
    case 'Agree':
      return [{ type: 'Registration', data: { request, step: { type: 'Token' } } }];
    case 'CheckCode':
      return [
        {
          type: 'QrLogin',
          data: { request, step: { type: 'Approve', data: { url: ACCOUNT_PAGE } } },
        },
      ];
    case 'RegistrationToken':
      return [{ type: 'Registration', data: { request, step: { type: 'Email' } } }];
    case 'Email':
      return [
        {
          type: 'Registration',
          data: {
            request,
            step: { type: 'EmailSent', data: { address: answer.data, unconfirmed: false } },
          },
        },
      ];
    case 'Continue':
    case 'LocationFix':
    // Widget runs' answers (`widgetAnswered`).
    case 'WidgetMessage':
    case 'WidgetPermissions':
    case 'WidgetLoading':
    case 'WidgetLoaded':
      return [];
  }
}

const POLICIES = [
  { name: 'Terms of service', url: 'https://example.org/terms' },
  { name: 'Privacy policy', url: 'https://example.org/privacy' },
];

const ACCOUNT_PAGE = 'https://account.hobby.chat/link';

/**
 * A room as a link shows it: #general (joined) by its alias or ID, #lab's successor, else a public
 * room nobody joined.
 */
function demoPreview(room: string): RoomPreview {
  if (room === LAB_V2) {
    return {
      room_id: LAB_V2,
      alias: null,
      name: 'lab',
      topic: 'My own scratch room',
      avatar: null,
      member_count: 1,
      join_rule: 'Invite',
      space: false,
      joined: false,
      invited: false,
    };
  }

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
      return { type: 'Text', data: { body: message.data.body, html: message.data.html } };
    case 'Emote':
      return { type: 'Emote', data: { body: message.data.body, html: message.data.html } };
    case 'Location':
      return { type: 'Location', data: { ...message.data, map: mapOf(message.data.geo_uri) } };
    case 'Gif':
      // A GIF goes as its link, as the engine sends it.
      return { type: 'Text', data: { body: message.data.url, html: null } };
    case 'Sticker':
      // A sticker shows as an image drawn on its own, as the engine reads one.
      return {
        type: 'Attachment',
        data: {
          kind: 'Image',
          name: message.data.body,
          size: null,
          media: message.data.media,
          thumbnail: null,
          dimensions: message.data.dimensions,
          audio: null,
          sticker: true,
        },
      };
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
          sticker: false,
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
  /** Sign-ins waiting for an answer or a cancel. */
  const waiting = new Set<RequestId>();
  /** Widget runs showing their page, until cancelled. */
  const widgetRuns = new Set<RequestId>();
  /** Drafts kept as the engine's store keeps them, for the run of the demo. */
  const drafts = new Map<string, Draft>();
  /** What the server holds to send later, by account (a scene's own list included). */
  const scheduled = new Map<string, ScheduledMessage[]>();
  /** Each account's safety choices, as its account data holds them. */
  const safeties = new Map<string, Safety>();
  /** Rooms' settings and member lists as changes left them, by account and room. */
  const settings = new Map<string, RoomSettings>();
  const members = new Map<string, Members>();
  /** Each account's image packs as edits left them. */
  const packs = new Map<string, ImagePack[]>();
  const packsOf = (account: string) => packs.get(account) ?? demoPacks();
  engine.subscribe((event) => {
    if (event.type === 'RoomsUpdated') lists.set(event.data.account, event.data.list);

    if (event.type === 'ScheduledMessages') scheduled.set(event.data.account, event.data.messages);

    if (event.type === 'Safety') safeties.set(event.data.account, event.data.safety);
  });

  const tellScheduled = (account: string) =>
    engine.emit({
      type: 'ScheduledMessages',
      data: { account, messages: scheduled.get(account) ?? [] },
    });

  const roomKeyOf = (account: string, roomId: string) => `${account}|${roomId}`;

  /** The account's room list after `change`, reported as the engine reports it. */
  const updateList = (account: string, change: (list: RoomList) => RoomList) => {
    const list = lists.get(account);

    if (list) engine.emit({ type: 'RoomsUpdated', data: { account, list: change(list) } });
  };

  /** Starts a room's settings with what it was made or joined with. */
  const seed = (account: string, roomId: string, start: Partial<RoomSettings>): void => {
    const made = settingsOf(account, roomId);

    if (made) settings.set(roomKeyOf(account, roomId), { ...made, ...start });
  };

  const joinRuleOf = (account: string, roomId: string, rule: JoinRule): void =>
    seed(account, roomId, { join_rule: rule });

  /** A room's or space's settings as last changed, from the fixtures at first. */
  const settingsOf = (account: string, roomId: string): RoomSettings | null => {
    const known = settings.get(roomKeyOf(account, roomId));

    if (known) return known;
    const list = lists.get(account);
    const room = list?.rooms.find((r) => r.room_id === roomId);
    const space = list?.spaces.find((sp) => sp.room_id === roomId);

    const made = room
      ? demoRoomSettings(room, false)
      : space
        ? demoRoomSettings(space, true)
        : null;

    if (made) settings.set(roomKeyOf(account, roomId), made);

    return made;
  };

  const membersOf = (account: string, roomId: string): Members =>
    members.get(roomKeyOf(account, roomId)) ?? demoMembers(roomId);

  /** A new group room, as the room list shows it. */
  const newRoom = (roomId: string, name: string, topic: string | null): RoomSummary => ({
    room_id: roomId,
    name,
    topic,
    kind: 'Group',
    unread: 0,
    mentions: 0,
    dm_users: [],
    note_to_self: false,
    avatar: null,
    member_count: 1,
    active_member_count: 1,
    history: 'Shared',
    notifications: 'AllMessages',
    custom_notifications: false,
    call: { via: 'Rtc', participants: [] },
    activity_ms: null,
    encrypted: true,
    pinned: [],
    can_pin: true,
    saved: [],
    can_start_meeting: false,
    can_rename: true,
    can_change_settings: true,
    knocks: 0,
    successor: null,
    priority: 'Normal',
    marked_unread: false,
    public: false,
  });

  /** Whether a room was replaced by an upgrade: its levels are closed, it isn't upgraded again. */
  const replaced = (account: string, roomId: string): boolean =>
    lists.get(account)?.rooms.find((r) => r.room_id === roomId)?.successor != null;

  /** A replaced room's settings as the engine reads them: nothing to upgrade or re-level. */
  const sealed = (found: RoomSettings): RoomSettings => ({
    ...found,
    upgrade_to: null,
    rights: { ...found.rights, upgrade: false },
    permissions: found.permissions.map((p) => ({ ...p, editable: false })),
  });

  /**
   * Answers the administration commands, as a homeserver would: `true` when done, the error when
   * refused, `false` for any other command.
   */
  function administer(command: Command, request: RequestId): boolean | string {
    switch (command.type) {
      case 'ListMembers': {
        const { account, room_id: roomId } = command.data;
        engine.emit({
          type: 'Members',
          data: { request: null, account, room_id: roomId, members: membersOf(account, roomId) },
        });

        return true;
      }

      case 'ModerateMember': {
        const { account, room_id: roomId, user_id: userId, action } = command.data;
        const before = membersOf(account, roomId);
        const asking = before.knocking.find((m) => m.person.id === userId);

        const level =
          action.type === 'Level'
            ? action.data
            : action.type === 'Role'
              ? roleLevel(action.data)
              : null;

        const after: Members = {
          ...before,
          people:
            action.type === 'Admit' && asking
              ? [...before.people, asking]
              : action.type === 'Kick' || action.type === 'Ban'
                ? before.people.filter((m) => m.person.id !== userId)
                : level === null
                  ? before.people
                  : before.people.map((m) =>
                      m.person.id === userId
                        ? {
                            ...m,
                            level,
                            role: level >= 100 ? 'Admin' : level >= 50 ? 'Moderator' : 'Member',
                          }
                        : m,
                    ),
          knocking:
            action.type === 'Admit' || action.type === 'Deny'
              ? before.knocking.filter((m) => m.person.id !== userId)
              : before.knocking,
          banned:
            action.type === 'Unban'
              ? before.banned.filter((m) => m.person.id !== userId)
              : action.type === 'Ban'
                ? [...before.banned, ...before.people.filter((m) => m.person.id === userId)]
                : before.banned,
        };

        members.set(roomKeyOf(account, roomId), after);
        engine.emit({ type: 'MembersInvalidated', data: { account, room_id: roomId } });
        updateList(account, (list) => ({
          ...list,
          rooms: list.rooms.map((r) =>
            r.room_id === roomId ? { ...r, knocks: after.knocking.length } : r,
          ),
        }));

        return true;
      }

      case 'LoadRoomSettings': {
        const { account, room_id: roomId } = command.data;
        const found = settingsOf(account, roomId);

        if (!found) return false;
        const shown = replaced(account, roomId) ? sealed(found) : found;
        engine.emit({ type: 'RoomSettings', data: { account, settings: shown } });

        return true;
      }

      case 'ChangeRoom': {
        const { account, room_id: roomId, setting } = command.data;
        const before = settingsOf(account, roomId);

        if (!before) return false;

        if (setting.type === 'Permission' && replaced(account, roomId)) {
          return 'this room was replaced already';
        }

        const after = withPending(
          setting.type === 'Avatar' && setting.data === null ? { ...before, avatar: null } : before,
          [setting],
        );

        settings.set(roomKeyOf(account, roomId), after);
        const shown = replaced(account, roomId) ? sealed(after) : after;
        engine.emit({ type: 'RoomSettings', data: { account, settings: shown } });
        updateList(account, (list) => ({
          ...list,
          rooms: list.rooms.map((r) =>
            r.room_id === roomId
              ? {
                  ...r,
                  name: after.name ?? r.name,
                  topic: after.topic,
                  avatar: after.avatar,
                  history: after.history,
                  encrypted: after.encrypted,
                  public: after.join_rule === 'Public',
                }
              : r,
          ),
          spaces: list.spaces.map((sp) =>
            sp.room_id === roomId
              ? { ...sp, name: after.name ?? sp.name, avatar: after.avatar }
              : sp,
          ),
        }));

        return true;
      }

      case 'UpgradeRoom': {
        const { account, room_id: roomId, version } = command.data;
        const next = `${roomId.split(':')[0]}-v${version}:example.org`;
        const old = lists.get(account)?.rooms.find((r) => r.room_id === roomId);

        if (!old) return false;

        if (old.successor) return 'this room was replaced already';
        // The new room keeps the old one's rule, as the server copies it.
        const rule = settingsOf(account, roomId)?.join_rule ?? (old.public ? 'Public' : 'Invite');
        engine.emit({ type: 'RoomCreated', data: { request, room_id: next } });
        updateList(account, (list) => ({
          ...list,
          rooms: [
            ...list.rooms.map((r) =>
              r.room_id === roomId
                ? { ...r, successor: { room_id: next, reason: null, via: ['example.org'] } }
                : r,
            ),
            { ...newRoom(next, old.name, old.topic), public: old.public },
          ],
          spaces: list.spaces.map((sp) => ({
            ...sp,
            children: sp.children.map((id) => (id === roomId ? next : id)),
          })),
        }));
        settings.delete(roomKeyOf(account, roomId));
        joinRuleOf(account, next, rule);

        return true;
      }

      case 'ChangeSpace': {
        const { account, space, change } = command.data;
        updateList(account, (list) => ({
          ...list,
          spaces: list.spaces.map((sp) => {
            if (sp.room_id !== space) return sp;

            switch (change.type) {
              case 'Add':
                return { ...sp, children: [...sp.children, change.data] };
              case 'Remove':
                return { ...sp, children: sp.children.filter((id) => id !== change.data) };
              case 'Order':
                return { ...sp, children: change.data };
            }
          }),
        }));

        return true;
      }

      case 'LeaveSpace': {
        const { account, space, rooms } = command.data;
        updateList(account, (list) => ({
          ...list,
          rooms: list.rooms.filter((r) => !rooms.includes(r.room_id)),
          spaces: list.spaces.filter((sp) => sp.room_id !== space && !rooms.includes(sp.room_id)),
        }));

        return true;
      }

      case 'CreateRoom': {
        const { account, room } = command.data;
        const roomId = `!${room.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${request}:example.org`;
        engine.emit({ type: 'RoomCreated', data: { request, room_id: roomId } });
        updateList(account, (list) =>
          room.space
            ? {
                ...list,
                spaces: [
                  ...list.spaces,
                  {
                    room_id: roomId,
                    name: room.name,
                    avatar: null,
                    can_add_rooms: true,
                    can_rename: true,
                    can_change_settings: true,
                    children: [],
                    members: [account],
                  },
                ],
              }
            : {
                ...list,
                rooms: [
                  ...list.rooms,
                  {
                    ...newRoom(roomId, room.name, room.topic),
                    encrypted: room.encrypted,
                    public: room.join_rule === 'Public',
                  },
                ],
                spaces: list.spaces.map((sp) =>
                  sp.room_id === room.parent ? { ...sp, children: [...sp.children, roomId] } : sp,
                ),
              },
        );

        // Its settings start as it was asked for.
        const address =
          room.join_rule === 'Public' && room.address ? `#${room.address}:example.org` : null;

        seed(account, roomId, {
          join_rule: room.join_rule,
          topic: room.topic,
          encrypted: !room.space && room.encrypted,
          main_address: address,
          addresses: [],
          published: room.join_rule === 'Public' && room.published,
        });

        return true;
      }

      case 'JoinRoom': {
        const { account, room_id: roomId } = command.data;
        const preview = demoPreview(roomId);
        engine.emit({ type: 'RoomJoined', data: { request, room_id: preview.room_id } });

        // A room already joined keeps its settings; a new one starts with the preview's rule.
        if (!lists.get(account)?.rooms.some((r) => r.room_id === preview.room_id)) {
          updateList(account, (list) => ({
            ...list,
            rooms: [
              ...list.rooms,
              {
                ...newRoom(preview.room_id, preview.name ?? preview.room_id, preview.topic),
                public: preview.join_rule === 'Public',
              },
            ],
          }));

          if (preview.join_rule) joinRuleOf(account, preview.room_id, preview.join_rule);
        }

        return true;
      }

      default:
        return false;
    }
  }

  const finish = (request: RequestId, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  const live = demoLiveLocations(engine, finish);

  async function respond(command: Command, request: RequestId) {
    if (live(command, request)) return;

    if (command.type === 'ListImagePacks') {
      const { account, room_id: roomId } = command.data;
      engine.emit({
        type: 'ImagePacks',
        data: {
          account,
          room_id: roomId,
          packs: packsFor(packsOf(account), roomId),
          may_add: true,
        },
      });
      finish(request);

      return;
    }

    if (command.type === 'EditImagePack') {
      const { account, pack, edit } = command.data;
      packs.set(account, editedPacks(packsOf(account), pack, edit, request));
      engine.emit({
        type: 'ImagePackEdited',
        data: { request, account, pack: editedPlace(pack, edit, request) },
      });
      finish(request);

      return;
    }

    if (command.type === 'SearchGifs') {
      const { key, query, offset } = command.data;

      if (key.trim() === '') {
        finish(request, 'GIF search needs a GIPHY API key');

        return;
      }

      const previews = new Map<string, string>();

      for (const gif of DEMO_GIFS) {
        const path = await resolveMedia(gif.media).catch(() => null);

        if (path) previews.set(gif.media, `file://${path}`);
      }

      engine.emit({
        type: 'GifsFound',
        data: { request, query, offset, page: demoGifPage(query, offset, previews) },
      });
      finish(request);

      return;
    }

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

    if (command.type === 'SetSafety') {
      const { account, change } = command.data;
      const safety = withChange(safeties.get(account) ?? DEFAULT_SAFETY, change);

      engine.emit({ type: 'Safety', data: { account, safety } });
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

      engine.emit({ type: 'DraftLoaded', data: { request, account, key, draft } });
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

    if (command.type === 'Schedule') {
      const { account, key, message, delay_ms: delay } = command.data;
      const body = message.type === 'Text' || message.type === 'Emote' ? message.data.body : null;

      const held: ScheduledMessage = {
        delay_id: `demo-delay-${request}`,
        room_id: key.room_id,
        send_at_ms: Date.now() + delay,
        body,
        failed: null,
      };

      scheduled.set(
        account,
        [...(scheduled.get(account) ?? []), held].sort((a, b) => a.send_at_ms - b.send_at_ms),
      );
      tellScheduled(account);
      finish(request);

      return;
    }

    if (command.type === 'ListScheduled') {
      tellScheduled(command.data.account);
      finish(request);

      return;
    }

    if (command.type === 'UpdateScheduled') {
      const { account, delay_id: delayId, action } = command.data;
      const held = (scheduled.get(account) ?? []).find((m) => m.delay_id === delayId);
      scheduled.set(
        account,
        (scheduled.get(account) ?? []).filter((m) => m.delay_id !== delayId),
      );

      if (held && action === 'SendNow') {
        for (const event of answer(
          {
            type: 'Send',
            data: {
              account,
              key: { room_id: held.room_id, thread: null },
              reply_to: null,
              message: {
                type: 'Text',
                data: {
                  body: held.body ?? '',
                  html: null,
                  mentions: { user_ids: [], room: false },
                },
              },
            },
          },
          request,
          Date.now(),
        ))
          engine.emit(event);
      }

      tellScheduled(account);
      finish(request);

      return;
    }

    if (command.type === 'FetchGif') {
      finish(request, 'not in the demo');

      return;
    }

    // Widget runs show their page until the app cancels them.
    const run = widgetRun(command, request);

    if (run !== null) {
      for (const event of run.events) engine.emit(event);

      if (run.waits) widgetRuns.add(request);
      else finish(request, run.error);

      return;
    }

    if (command.type === 'Answer' && widgetRuns.has(command.data.request)) {
      for (const event of widgetAnswered(command.data.answer, command.data.request))
        engine.emit(event);
      finish(request);

      return;
    }

    if (command.type === 'Cancel' && widgetRuns.delete(command.data)) {
      finish(command.data, 'cancelled');
      finish(request);

      return;
    }

    // Sign-ins that ask on the way wait, as on a server, until answered to the end or cancelled.
    const first = asking(command, request);

    if (first.length > 0) {
      for (const event of first) engine.emit(event);
      waiting.add(request);

      return;
    }

    const administered = administer(command, request);

    if (administered !== false) {
      finish(request, administered === true ? undefined : administered);

      return;
    }

    if (command.type === 'Answer' && waiting.has(command.data.request)) {
      const next = nextAsked(command.data.answer, command.data.request);

      for (const event of next) engine.emit(event);

      // Nothing more to ask: the demo has no account to sign in.
      if (next.length === 0) {
        waiting.delete(command.data.request);
        finish(command.data.request, 'not in the demo');
      }
    } else if (command.type === 'Cancel' && waiting.delete(command.data)) {
      finish(command.data, 'cancelled');
    }

    for (const event of answer(command, request, Date.now())) engine.emit(event);
    finish(request);
  }

  return engine;
}
