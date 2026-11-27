// Sample accounts for demo scenes, ported from the desktop's `apps/desktop/src/demo.rs`: the same
// people, rooms and messages, as engine events. They reach the state through `apply`, like real
// events, so a scene exercises the same path the engine does (ADR 0002).

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  AccountId,
  Attachment,
  CallInfo,
  EditVersion,
  ImagePack,
  EventSource,
  IncomingCall,
  Invitation,
  MemberRole,
  Members,
  Message,
  MessageContent,
  PackImage,
  Identity,
  Permission,
  Person,
  RoomChange,
  Place,
  Presence,
  QrImage,
  RoomMember,
  RoomPreview,
  RoomSettings,
  RoomSummary,
  Safety,
  SasEmoji,
  Session,
  SharedFile,
  SpaceSummary,
  ThreadInfo,
  TimelineEntry,
  Meeting,
  Widget,
  WidgetPermission,
  Widgets,
} from '@better-matrix/matrix-native/types';

import { mapOf } from '../state/geo';
import { mxcMedia } from '../state/messages';

export const ADA = '@ada:example.org';

export const ADA_HOBBY = '@ada:hobby.chat';

export const GRACE = '@grace:example.org';

export const ALAN = '@alan:example.org';

export const KAI = '@kai:example.org';

const MIRA = '@mira:hobby.chat';

export const WORK_SPACE = '!engines:example.org';

export const GENERAL = '!general:example.org';

export const DESIGN = '!design:example.org';

/** A room Ada is invited to and hasn't joined. */
export const DESIGN_INVITE = '!design-community:example.org';

export const DM_GRACE = '!dm-grace:example.org';

/** Ada's scratch room: the room the upgrade scenes replace. */
export const LAB = '!lab:example.org';

/** The room an upgrade of #lab made, which Ada hasn't joined yet. */
export const LAB_V2 = '!lab-v12:example.org';

export const DM_KAI = '!dm-kai:example.org';

/** Ada's note to self. */
export const NOTES = '!notes:example.org';

/** Where releases are signed off: the room that shows every kind of message the composer sends. */
export const RELEASES = '!releases:example.org';

/** A public room nobody in the demo has joined, as a permalink to it previews. */
export const DEMO_LINKED_ROOM: RoomPreview = {
  room_id: '!rust-lang:matrix.org',
  alias: '#rust:matrix.org',
  name: 'Rust',
  topic: 'The Rust programming language. Be kind, and keep it on topic.',
  avatar: null,
  member_count: 4812,
  join_rule: 'Public',
  space: false,
  joined: false,
  invited: false,
};

/** The link with a card in #general. */
const RELEASE_NOTES = 'https://bettermatrix.example.org/blog/0.2';

/** The demo's day: 12 November 2026, UTC (the desktop's `DEMO_DAY_MS`). */
export const DEMO_DAY_MS = 20_769 * 86_400_000;

/** Media the demo engine serves from the app's own assets, by media ref. */
export const DEMO_MEDIA = {
  screenshot: 'demo-screenshot',
  release: 'demo-release-notes',
  voice: 'demo-voice-memo',
  /** A video the demo has no file for: its poster shows, the player waits. */
  walkthrough: 'demo-walkthrough',
} as const;

/** The demo's custom emoji and stickers, by `mxc://` URI, as the demo engine serves them. */
export const DEMO_PACK_IMAGES = {
  party: 'mxc://example.org/emote-party',
  ok: 'mxc://example.org/emote-ok',
  wave: 'mxc://example.org/sticker-wave',
  ship: 'mxc://example.org/sticker-ship',
  coffee: 'mxc://example.org/sticker-coffee',
} as const;

/** The voice memo Grace sends: 14 seconds of speech, with the waveform a client sends along. */
function voiceMemo(): MessageContent {
  // Words and the pauses between them, as the desktop's fixture: a swell every few bars,
  // quieter at the breaths.
  const waveform = Array.from({ length: 100 }, (_, i) => {
    const words = (Math.sin(i * 0.55) * 0.5 + 0.5) * (Math.sin(i * 0.13) * 0.35 + 0.65);
    const breath = Math.sin(i * 0.09) > 0.93 ? 0.1 : 1;

    return Math.floor(words * breath * 1000) + 24;
  });

  return {
    type: 'Attachment',
    data: file('voice-1765012345.wav', 'Audio', {
      size: 670_000,
      media: DEMO_MEDIA.voice,
      audio: { voice: true, duration_ms: 14_000, waveform },
    }),
  };
}

function person(id: string, name: string): Person {
  return { id, name, avatar: null };
}

function text(body: string): MessageContent {
  return { type: 'Text', data: { body, html: null } };
}

function html(body: string, markup: string): MessageContent {
  return { type: 'Text', data: { body, html: markup } };
}

function callCard(video: boolean | null): MessageContent {
  return { type: 'Call', data: { video, direct: false, declined_by: [] } };
}

function file(name: string, kind: Attachment['kind'], extra: Partial<Attachment> = {}): Attachment {
  return {
    kind,
    name,
    size: 1_450_000,
    media: null,
    thumbnail: null,
    dimensions: null,
    audio: null,
    sticker: false,
    ...extra,
  };
}

function blank(): Omit<
  Message,
  'event_id' | 'sender_id' | 'sender_name' | 'content' | 'timestamp_ms'
> {
  return {
    sender_avatar: null,
    edited: false,
    editable: false,
    pending: false,
    reactions: [],
    reply_to: null,
    thread: null,
    highlighted: false,
  };
}

function emote(body: string, markup: string | null = null): MessageContent {
  return { type: 'Emote', data: { body, html: markup } };
}

function place(geoUri: string, description: string | null): MessageContent {
  return { type: 'Location', data: { geo_uri: geoUri, description, map: mapOf(geoUri) } };
}

/** The Brandenburg Gate, where the release party is. */
const PARTY_PLACE = 'geo:52.516275,13.377704';

/** Ada's sign-off in #releases, edited twice: the edit history scene opens it. */
export const SIGN_OFF = `$demo-${ADA}-5`;

/** Kai's link in #releases to Alan's answer inside #general's thread. */
export const THREAD_LINK = `$demo-${KAI}-6`;

/** The answer Kai's link points at: a reply inside #general's thread. */
export const THREAD_REPLY = '$thread-1';

/**
 * #releases: Alan mentions Ada (highlighted), Grace's emote and the party's place, Kai's spoiler
 * and formula, Ada's sign-off with a custom emoji (edited), and Kai's link into a thread.
 */
function releases(): Message[] {
  const party = `<img data-mx-emoticon src="${DEMO_PACK_IMAGES.party}" alt=":party:" title=":party:" height="32" />`;

  const list = messages([
    [
      ALAN,
      'Alan Turing',
      html(
        'Ada Lovelace can you sign off 0.2.0?',
        `<a href="https://matrix.to/#/${ADA}">Ada Lovelace</a> can you sign off 0.2.0?`,
      ),
      0,
    ],
    [GRACE, 'Grace Hopper', emote('is writing the release notes'), 3],
    [GRACE, 'Grace Hopper', place(PARTY_PLACE, 'Release party: Brandenburger Tor'), 2],
    [
      KAI,
      'Kai',
      html(
        'Guess the number of fixes: [Spoiler]. Our favourite formula: e^{i\\pi} + 1 = 0',
        'Guess the number of fixes: <span data-mx-spoiler>42</span>. Our favourite formula: ' +
          '<span data-mx-maths="e^{i\\pi} + 1 = 0"><code>e^{i\\pi} + 1 = 0</code></span>',
      ),
      4,
    ],
    [
      GRACE,
      'Grace Hopper',
      html(
        '@room the tag goes out at 15:00 :ok:',
        `@room the tag goes out at 15:00 <img data-mx-emoticon src="${DEMO_PACK_IMAGES.ok}" alt=":ok:" title=":ok:" height="32" />`,
      ),
      1,
    ],
    [
      ADA,
      'Ada Lovelace',
      html('Signed off 0.2.0 :party:', `<p>Signed off <strong>0.2.0</strong> ${party}</p>`),
      6,
    ],
    [
      KAI,
      'Kai',
      text(`Alan answered this in the thread: https://matrix.to/#/${GENERAL}/${THREAD_REPLY}`),
      2,
    ],
  ]);

  list[0] = { ...list[0]!, highlighted: true };
  list[4] = { ...list[4]!, highlighted: true };
  list[5] = { ...list[5]!, edited: true };

  return list;
}

/** The versions of Ada's sign-off, the original first. */
export function signOffVersions(): EditVersion[] {
  const at = DEMO_DAY_MS + (9 * 60 + 28) * 60_000;

  return [
    { event_id: SIGN_OFF, body: 'Signed off 0.2', html: null, timestamp_ms: at },
    {
      event_id: '$sign-off-edit-1',
      body: 'Signed off 0.2.0',
      html: '<p>Signed off <strong>0.2.0</strong></p>',
      timestamp_ms: at + 2 * 60_000,
    },
    {
      event_id: '$sign-off-edit-2',
      body: 'Signed off 0.2.0 :party:',
      html: `<p>Signed off <strong>0.2.0</strong> <img data-mx-emoticon src="${DEMO_PACK_IMAGES.party}" alt=":party:" title=":party:" height="32" /></p>`,
      timestamp_ms: at + 5 * 60_000,
    },
  ];
}

function packImage(
  shortcode: string,
  body: string,
  url: string,
  use: { emoticon: boolean; sticker: boolean },
): PackImage {
  return {
    shortcode,
    body,
    url,
    media: mxcMedia(url),
    ...use,
    dimensions: use.sticker ? [256, 256] : [128, 128],
    mimetype: 'image/png',
  };
}

/** Ada's own pack (two custom emoji) and #general's sticker pack. */
export function demoPacks(): ImagePack[] {
  const emoticon = { emoticon: true, sticker: false };
  const sticker = { emoticon: false, sticker: true };

  return [
    {
      id: 'user',
      name: 'Ada’s emoji',
      avatar: null,
      avatar_url: null,
      place: { type: 'User' },
      editable: true,
      everywhere: false,
      images: [
        packImage('party', 'Party', DEMO_PACK_IMAGES.party, emoticon),
        packImage('ok', 'OK', DEMO_PACK_IMAGES.ok, emoticon),
      ],
    },
    {
      id: `${GENERAL}/`,
      name: 'Engine room',
      avatar: null,
      avatar_url: DEMO_PACK_IMAGES.wave,
      place: { type: 'Room', data: { room_id: GENERAL, state_key: '' } },
      editable: true,
      everywhere: false,
      images: [
        packImage('wave', 'Waving hello', DEMO_PACK_IMAGES.wave, sticker),
        packImage('ship', 'Ship it', DEMO_PACK_IMAGES.ship, sticker),
        packImage('coffee', 'Coffee break', DEMO_PACK_IMAGES.coffee, sticker),
      ],
    },
  ];
}

/** Places found by name, as OpenStreetMap's search answers. */
export function demoPlaces(query: string): Place[] {
  const all: Array<[string, string]> = [
    ['Brandenburger Tor, Pariser Platz, Mitte, Berlin', PARTY_PLACE],
    ['Fernsehturm, Panoramastraße, Mitte, Berlin', 'geo:52.520815,13.409419'],
    ['Tempelhofer Feld, Tempelhof, Berlin', 'geo:52.473055,13.40368'],
  ];

  const words = query.toLowerCase().split(/\s+/).filter(Boolean);

  return all.flatMap(([name, geoUri]) =>
    words.some((word) => name.toLowerCase().includes(word))
      ? [{ name, geo_uri: geoUri, map: mapOf(geoUri) }]
      : [],
  );
}

/** Messages `gap` minutes after the one before, from 09:12 UTC on the demo's day. */
function messages(lines: Array<[string, string, MessageContent, number]>): Message[] {
  let at = DEMO_DAY_MS + (9 * 60 + 12) * 60_000;

  return lines.map(([sender, name, content, gap], i) => {
    at += gap * 60_000;

    return {
      ...blank(),
      event_id: `$demo-${sender}-${i}`,
      sender_id: sender,
      sender_name: name,
      content,
      timestamp_ms: at,
      // Ada may edit her own plain text, as the engine reports it.
      editable: sender === ADA || sender === ADA_HOBBY ? content.type === 'Text' : false,
    };
  });
}

function room(
  id: string,
  name: string,
  topic: string | null,
  unread: number,
  mentions: number,
): RoomSummary {
  return {
    room_id: id,
    name,
    name_from_member: false,
    topic,
    kind: 'Group',
    unread,
    mentions,
    dm_users: [],
    note_to_self: false,
    avatar: null,
    member_count: 4,
    active_member_count: 4,
    history: 'Shared',
    notifications: 'AllMessages',
    custom_notifications: false,
    // The demo's homeserver names a call service.
    call: { via: 'Rtc', participants: [] },
    activity_ms: null,
    encrypted: true,
    public: false,
    pinned: id === GENERAL ? [DEMO_THREAD.root, TEST_PLAN] : [],
    can_pin: true,
    saved: id === GENERAL ? [TEST_PLAN] : [],
    // The demo's homeserver names a Jitsi server; #general has a meeting pinned, #design none.
    can_start_meeting: id === GENERAL || id === DESIGN,
    can_share_live_location: true,
    can_rename: true,
    priority: 'Normal',
    marked_unread: false,
    can_change_settings: true,
    knocks: 0,
    successor: null,
  };
}

/** The user's note to self, as the engine reports one: listed with the direct chats, no calls. */
function noteToSelf(id: string): RoomSummary {
  return {
    ...room(id, 'Note to self', null, 0, 0),
    kind: 'Direct',
    note_to_self: true,
    member_count: 1,
    active_member_count: 1,
    call: { via: null, participants: [] },
  };
}

function dm(id: string, withId: string, name: string, unread: number): RoomSummary {
  return {
    ...room(id, name, null, unread, unread),
    kind: 'Direct',
    name_from_member: true,
    dm_users: [withId],
  };
}

function space(id: string, name: string, children: string[], members: string[]): SpaceSummary {
  return {
    room_id: id,
    name,
    avatar: null,
    can_add_rooms: true,
    can_rename: true,
    can_change_settings: true,
    children,
    members,
  };
}

/**
 * How a room begins: created and set up (changes only the room log lists by default), an invite,
 * a run of joins that reads as one line, then a new topic and a new profile picture. Each a
 * minute after the one before, the last a minute before `before`.
 */
function roomEvents(name: string, before: number): TimelineEntry[] {
  const ada = person(ADA, 'Ada Lovelace');

  const joined: Array<[string, string]> = [
    [GRACE, 'Grace Hopper'],
    [ALAN, 'Alan Turing'],
    [KAI, 'Kai'],
    ['@mira:example.org', 'Mira'],
    ['@ken:example.org', 'Ken Thompson'],
    ['@barbara:example.org', 'Barbara Liskov'],
  ];

  const events: Array<[string, Person, RoomChange]> = [
    ['ev-created', ada, { type: 'Created' }],
    ['ev-name', ada, { type: 'RoomName', data: name }],
    ['ev-encrypted', ada, { type: 'Encrypted' }],
    ['ev-invited', ada, { type: 'Invited', data: person(KAI, 'Kai') }],
    ...joined.map(([id, member]): [string, Person, RoomChange] => [
      `ev-joined-${id}`,
      person(id, member),
      { type: 'Joined' },
    ]),
    ['ev-topic', ada, { type: 'RoomTopic', data: 'Where the team talks' }],
    ['ev-picture', person(GRACE, 'Grace Hopper'), { type: 'ProfilePicture' }],
  ];

  return events.map(([id, sender, change], index) => ({
    id,
    kind: {
      type: 'RoomEvent',
      data: { sender, change, timestamp_ms: before - (events.length - index) * 60_000 },
    },
  }));
}

/** A whole timeline: its start, the day's divider, the room's first events, then the messages. */
export function timelineEntries(name: string, list: Message[], unread = 0): TimelineEntry[] {
  const first = list[0]?.timestamp_ms ?? DEMO_DAY_MS;

  const messages = list.map((m): TimelineEntry => ({
    id: m.event_id,
    kind: { type: 'Message', data: m },
  }));

  // The SDK's read marker stands before the unread messages, as in a real unread room.
  const read = unread > 0 ? Math.max(0, messages.length - unread) : messages.length;

  const marker: TimelineEntry[] =
    read < messages.length ? [{ id: 'read-marker', kind: { type: 'ReadMarker' } }] : [];

  return [
    { id: 'start', kind: { type: 'Start' } },
    { id: 'day', kind: { type: 'DateDivider', data: first - (first % 86_400_000) } },
    ...roomEvents(name, first),
    ...messages.slice(0, read),
    ...marker,
    ...messages.slice(read),
  ];
}

/** Grace's poll on when to release: open, results showing, Ada's vote on Friday. */
function releasePoll(): MessageContent {
  const answer = (id: string, label: string, votes: number, mine: boolean) => ({
    id,
    text: label,
    votes,
    mine,
  });

  return {
    type: 'Poll',
    data: {
      question: 'When do we release 0.2?',
      answers: [
        answer('0', 'Friday', 2, true),
        answer('1', 'Monday', 1, false),
        answer('2', 'After the 0.3 fixes', 0, false),
      ],
      disclosed: true,
      max_selections: 1,
      voters: 3,
      ended: false,
    },
  };
}

interface Fixture {
  rooms: Array<[RoomSummary, Message[]]>;
  spaces: SpaceSummary[];
  /** Rooms and spaces the account is invited to. */
  invitations?: Invitation[];
  /** Threads: the room, the root message, and its replies. */
  threads: Array<[string, Message, Message[]]>;
}

/** The general room's messages, shared with the thread and the scenes that point into it. */
function general(): Message[] {
  const list = messages([
    [GRACE, 'Grace Hopper', text('Morning all! The staging homeserver is back up.'), 0],
    [GRACE, 'Grace Hopper', text('Federation to the public servers works again too.'), 1],
    [
      ALAN,
      'Alan Turing',
      html(
        'Encryption test plan:\nCross-signing\nKey backup\nRun cargo test -p matrix-core first.',
        '<p><b>Encryption test plan:</b></p><ul><li>Cross-signing</li><li>Key backup</li></ul><p>Run <code>cargo test -p matrix-core</code> first.</p>',
      ),
      6,
    ],
    [ADA, 'Ada Lovelace', { type: 'Attachment', data: file('release-notes-0.2.pdf', 'File') }, 14],
    [
      GRACE,
      'Grace Hopper',
      text('Nice! Looks good to ship. https://giphy.com/gifs/ship-it-l0MYC0LajbaPoEADu'),
      20,
    ],
    [
      ALAN,
      'Alan Turing',
      text('Cross-signing verification passes on Linux, macOS and Windows.'),
      22,
    ],
    [GRACE, 'Grace Hopper', text("Great, then let's tag it."), 2],
    [GRACE, 'Grace Hopper', releasePoll(), 3],
    [ALAN, 'Alan Turing', text(`The release notes are up: ${RELEASE_NOTES}`), 30],
    [ADA, 'Ada Lovelace', text('Thanks everyone, tagging it after lunch.'), 4],
  ]);

  const plan = list[2]!;
  // Reactions on the test plan, one of them Ada's own.
  list[2] = {
    ...plan,
    reactions: [
      { key: '👍', count: 3, mine: true },
      { key: '🎉', count: 1, mine: false },
    ],
  };
  // A reply quoting the test plan.
  list[6] = {
    ...list[6]!,
    reply_to: {
      event_id: plan.event_id,
      sender_name: 'Alan Turing',
      body: 'Encryption test plan: Cross-signing Key backup Run cargo test -p matrix-core first.',
      call: null,
    },
  };
  // Ada's last word, read by six (three the room knows only by ID), as the desktop's
  // read-receipts scene shows it.
  const last = list[9]!;
  list[9] = {
    ...last,
    read_receipts: [
      KAI,
      GRACE,
      ALAN,
      '@mira:example.org',
      '@lin:example.org',
      '@sam:example.org',
    ].map((user_id, index) => ({
      user_id,
      timestamp_ms: index === 5 ? null : last.timestamp_ms + (6 - index) * 60_000,
    })),
  };
  // A thread on the cross-signing report.
  const report = list[5]!;
  list[5] = {
    ...report,
    thread: {
      replies: 3,
      latest: person(GRACE, 'Grace Hopper'),
      latest_at: report.timestamp_ms + 21 * 60_000,
    },
  };

  return list;
}

/** The thread demo scenes open: Alan's cross-signing report in #general. */
export const DEMO_THREAD = { roomId: GENERAL, root: `$demo-${ALAN}-5` } as const;

/** Alan's encryption test plan in #general: formatted, with reactions and a reply. */
export const TEST_PLAN = `$demo-${ALAN}-2`;

/** Ada's last message in #general, which six people have read. */
export const READ_BY_SIX = `$demo-${ADA}-9`;

/** Grace's screenshot in #design, an image the demo engine serves. */
export const SCREENSHOT = `$demo-${GRACE}-1`;

/** The batch Grace posts into #design after Ada asks for the review's material. */
export const DEMO_BATCH = {
  first: '$batch-0',
  second: '$batch-1',
  video: '$batch-2',
  audio: '$batch-3',
} as const;

/**
 * Ada asks in #design, and Grace answers with four attachments in a row (two images, a video
 * and an audio clip): one batch, which the preview shows as a gallery.
 */
export function batchEvents(): EngineEvent[] {
  const at = (minutes: number) => DEMO_DAY_MS + (9 * 60 + minutes) * 60_000;

  const grace = (id: string, minutes: number, attachment: Attachment): Message => ({
    ...blank(),
    event_id: id,
    sender_id: GRACE,
    sender_name: 'Grace Hopper',
    content: { type: 'Attachment', data: attachment },
    timestamp_ms: at(minutes),
  });

  const list: Message[] = [
    {
      ...blank(),
      event_id: '$batch-ask',
      sender_id: ADA,
      sender_name: 'Ada Lovelace',
      content: text('Could you share what we went through in the review?'),
      timestamp_ms: at(20),
      editable: true,
    },
    grace(
      DEMO_BATCH.first,
      24,
      file('sidebar-before.png', 'Image', {
        media: DEMO_MEDIA.screenshot,
        dimensions: [1024, 1024],
        size: 346_467,
      }),
    ),
    grace(
      DEMO_BATCH.second,
      24,
      file('sidebar-after.png', 'Image', {
        media: DEMO_MEDIA.release,
        dimensions: [1200, 630],
        size: 512_204,
      }),
    ),
    grace(
      DEMO_BATCH.video,
      25,
      file('walkthrough.mp4', 'Video', {
        media: DEMO_MEDIA.walkthrough,
        thumbnail: DEMO_MEDIA.release,
        dimensions: [1280, 720],
        size: 8_400_000,
      }),
    ),
    grace(
      DEMO_BATCH.audio,
      26,
      file('review-notes.m4a', 'Audio', {
        media: DEMO_MEDIA.voice,
        size: 670_000,
        audio: { voice: false, duration_ms: 14_000, waveform: [] },
      }),
    ),
  ];

  return [
    {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: { room_id: DESIGN, thread: null },
        diffs: list.map((m) => ({
          type: 'PushBack' as const,
          data: { id: m.event_id, kind: { type: 'Message' as const, data: m } },
        })),
      },
    },
  ];
}

/** The sticker Grace sends into #general after the sign-off. */
export const STICKER_MESSAGE = '$demo-sticker';

/** Grace answers in #general with a sticker from the room's pack. */
export function stickerEvents(): EngineEvent[] {
  const message: Message = {
    ...blank(),
    event_id: STICKER_MESSAGE,
    sender_id: GRACE,
    sender_name: 'Grace Hopper',
    content: {
      type: 'Attachment',
      data: file('Ship it', 'Image', {
        media: mxcMedia(DEMO_PACK_IMAGES.ship),
        dimensions: [256, 256],
        size: null,
        sticker: true,
      }),
    },
    timestamp_ms: DEMO_DAY_MS + (9 * 60 + 40) * 60_000,
  };

  return [
    {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: { room_id: GENERAL, thread: null },
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

function work(): Fixture {
  const room0 = general();

  const thread = messages([
    [GRACE, 'Grace Hopper', text('Including recovery from the key backup?'), 80],
    [ALAN, 'Alan Turing', text('Yes, restored on a fresh Windows install.'), 3],
    [GRACE, 'Grace Hopper', text('Perfect, thanks!'), 1],
  ]).map((m, i) => ({ ...m, event_id: `$thread-${i}` }));

  return {
    rooms: [
      [room(GENERAL, 'general', 'Company-wide announcements and chatter', 0, 0), room0],
      [
        // Public: anyone may join and post, so the account's media choice may hold its
        // pictures back (ADR 0011). People ask to join it: its member list shows them to Ada.
        {
          ...room(DESIGN, 'design', 'Mockups, reviews and the component library', 3, 0),
          public: true,
          knocks: 2,
        },
        messages([
          [GRACE, 'Grace Hopper', text('New sidebar spacing is ready for review.'), 0],
          [
            GRACE,
            'Grace Hopper',
            {
              type: 'Attachment',
              data: file('screenshot-sidebar.png', 'Image', {
                media: DEMO_MEDIA.screenshot,
                dimensions: [1024, 1024],
                size: 346_467,
              }),
            },
            1,
          ],
        ]),
      ],
      [{ ...room(RELEASES, 'releases', null, 1, 1), history: 'Joined' }, releases()],
      [
        {
          ...room('!meetup:example.org', 'meetup', 'Monthly Rust meetup', 4, 0),
          notifications: 'MentionsOnly',
          custom_notifications: true,
        },
        [],
      ],
      // Organised as the desktop's demo: lab among the low priority rooms, Grace's chat a
      // favourite, Alan's muted chat marked unread.
      [{ ...room(LAB, 'lab', 'My own scratch room', 0, 0), priority: 'Low' }, []],
      [
        { ...dm(DM_GRACE, GRACE, 'Grace Hopper', 2), priority: 'Favourite' },
        messages([
          [ADA, 'Ada Lovelace', callCard(true), 0],
          [GRACE, 'Grace Hopper', text('Do you have a minute to look at the sync loop?'), 1],
          [GRACE, 'Grace Hopper', text('It drops the room list after a reconnect.'), 2],
          [GRACE, 'Grace Hopper', voiceMemo(), 1],
        ]),
      ],
      [
        {
          ...dm('!dm-alan:example.org', ALAN, 'Alan Turing', 0),
          notifications: 'Mute',
          marked_unread: true,
        },
        [],
      ],
      // Kai is calling: the card shows the call under way, with the way in.
      [
        { ...dm(DM_KAI, KAI, 'Kai', 1), call: { via: 'Rtc', participants: [KAI] } },
        messages([[KAI, 'Kai', callCard(false), 0]]),
      ],
      [
        noteToSelf(NOTES),
        messages([
          [ADA, 'Ada Lovelace', text('Ask Grace whether the reconnect fix made the release'), 0],
          [ADA, 'Ada Lovelace', text('Slides for the meetup: keep the engine diagram'), 0],
        ]),
      ],
    ],
    spaces: [
      space(WORK_SPACE, 'Analytical Engines', [GENERAL, DESIGN, RELEASES], [ADA, GRACE, ALAN]),
      space('!rust:example.org', 'Rust Meetup', ['!meetup:example.org'], [ADA, KAI]),
    ],
    threads: [[DEMO_THREAD.roomId, room0[5]!, thread]],
    // Grace invites Ada to the community's design room, as the desktop's demo does.
    invitations: [
      {
        room_id: DESIGN_INVITE,
        name: 'Design',
        topic: 'A place to share ideas, review designs, and build Better Matrix together.',
        avatar: null,
        inviter: person(GRACE, 'Grace Hopper'),
        space: false,
        kind: 'Group',
        stranger: false,
      },
    ],
  };
}

/** Someone Ada shares no room with, ignored in the safety scenes. */
export const SPAMMER = '@prizes:spam.example';

/** An invitation from a stranger, which the safety scenes hide. */
export const STRANGER_INVITE = '!free-crypto:spam.example';

/**
 * The safety scenes: Ada hides strangers' invitations (one waits, from someone she shares no
 * room with), shows media in private rooms only, and ignores a spammer.
 */
export function safetyEvents(): EngineEvent[] {
  const fixture = work();

  const safety: Safety = {
    ignored: [SPAMMER],
    media: 'Private',
    invite_pictures: false,
    hide_stranger_invites: true,
  };

  return [
    {
      type: 'RoomsUpdated',
      data: {
        account: ADA,
        list: {
          rooms: fixture.rooms.map(([r]) => r),
          spaces: fixture.spaces,
          invitations: [
            ...(fixture.invitations ?? []),
            {
              room_id: STRANGER_INVITE,
              name: 'Free crypto giveaway',
              topic: 'Claim your prize now!',
              avatar: null,
              inviter: person(SPAMMER, 'Prize Desk'),
              space: false,
              kind: 'Group',
              stranger: true,
            },
          ],
        },
      },
    },
    { type: 'Safety', data: { account: ADA, safety } },
  ];
}

function hobby(): Fixture {
  return {
    rooms: [
      [
        room('!synths:hobby.chat', 'synths', 'Modular, analog and everything with knobs', 5, 0),
        messages([[MIRA, 'mira', text('Finally finished the eurorack case build.'), 0]]),
      ],
      [
        {
          ...room('!bikes:hobby.chat', 'bikes', null, 7, 0),
          notifications: 'Mute',
          custom_notifications: true,
        },
        [],
      ],
      [dm('!dm-mira:hobby.chat', MIRA, 'mira', 1), []],
    ],
    spaces: [space('!synth-club:hobby.chat', 'Synth Club', ['!synths:hobby.chat'], [MIRA])],
    threads: [],
  };
}

function accountEvents(
  id: AccountId,
  name: string,
  server: string,
  signIn: 'Password' | 'OAuth',
  fixture: Fixture,
): EngineEvent[] {
  const events: EngineEvent[] = [
    {
      type: 'AccountAdded',
      data: {
        id,
        display_name: name,
        homeserver: server,
        default_notifications: 'AllMessages',
        avatar: null,
        sign_in: signIn,
      },
    },
    {
      type: 'RoomsUpdated',
      data: {
        account: id,
        list: {
          rooms: fixture.rooms.map(([r]) => r),
          spaces: fixture.spaces,
          invitations: fixture.invitations ?? [],
        },
      },
    },
    { type: 'SidebarLayout', data: { account: id, layout: { scopes: {}, changed_at: 0 } } },
    { type: 'Keywords', data: { account: id, keywords: ['deploy', 'release'] } },
  ];

  for (const [summary, list] of fixture.rooms) {
    const last = list[list.length - 1];

    if (last)
      events.push({
        type: 'LatestMessage',
        data: { account: id, room_id: summary.room_id, message: last },
      });
    events.push({
      type: 'TimelineChanged',
      data: {
        account: id,
        key: { room_id: summary.room_id, thread: null },
        diffs: [{ type: 'Reset', data: timelineEntries(summary.name, list, summary.unread) }],
      },
    });
  }

  for (const [roomId, root, replies] of fixture.threads) {
    const entries: TimelineEntry[] = [
      { id: 'start', kind: { type: 'Start' } },
      { id: root.event_id, kind: { type: 'Message', data: root } },
      ...replies.map((m): TimelineEntry => ({
        id: m.event_id,
        kind: { type: 'Message', data: m },
      })),
    ];

    events.push({
      type: 'TimelineChanged',
      data: {
        account: id,
        key: { room_id: roomId, thread: root.event_id },
        diffs: [{ type: 'Reset', data: entries }],
      },
    });
  }

  return events;
}

function member(
  id: string,
  name: string,
  role: MemberRole,
  presence: Presence,
  verified: boolean,
): RoomMember {
  const level = role === 'Admin' ? 100 : role === 'Moderator' ? 50 : 0;

  return { person: person(id, name), role, presence, status_message: null, verified, level };
}

/**
 * Who is in a room (the same people fill every room the demo engine is asked about); #design
 * also lists two people asking to join and one banned.
 */
export function demoMembers(roomId?: string): Members {
  const design = roomId === DESIGN;

  return {
    people: [
      member(ADA, 'Ada Lovelace', 'Admin', 'Online', true),
      member(GRACE, 'Grace Hopper', 'Moderator', 'Online', true),
      member(KAI, 'Kai', 'Member', 'Away', false),
      member(ALAN, 'Alan Turing', 'Member', 'Offline', false),
    ],
    complete: true,
    can_invite: true,
    own_level: 100,
    can_kick: true,
    can_ban: true,
    can_set_roles: true,
    knocking: design
      ? [
          member('@lin:example.org', 'Lin Chen', 'Member', 'Online', false),
          member('@sam:hobby.chat', 'Sam Okafor', 'Member', 'Offline', false),
        ]
      : [],
    banned: design
      ? [member('@spam:example.net', 'Totally Real Offers', 'Member', 'Offline', false)]
      : [],
  };
}

/**
 * A room's or space's settings as Ada (an admin) sees them: #general is public with two
 * addresses, the others private; #lab can be upgraded.
 */
export function demoRoomSettings(
  summary: RoomSummary | SpaceSummary,
  space: boolean,
): RoomSettings {
  const general = summary.room_id === GENERAL;

  const listed: Permission[] = space
    ? [
        'DefaultLevel',
        'Invite',
        'Kick',
        'Ban',
        'ChangeSettings',
        'Name',
        'Topic',
        'Avatar',
        'Addresses',
        'Access',
        'SpaceRooms',
        'Permissions',
        'Upgrade',
      ]
    : [
        'DefaultLevel',
        'SendMessages',
        'Invite',
        'Kick',
        'Ban',
        'RemoveMessages',
        'MentionRoom',
        'Pins',
        'ChangeSettings',
        'Name',
        'Topic',
        'Avatar',
        'Addresses',
        'Access',
        'Encryption',
        'Permissions',
        'Upgrade',
      ];

  const levels: Partial<Record<Permission, number>> = {
    DefaultLevel: 0,
    SendMessages: 0,
    Invite: 0,
    RemoveMessages: 50,
    // The server's admins keep upgrades to themselves.
    Upgrade: 150,
  };

  return {
    room_id: summary.room_id,
    space,
    name: summary.name,
    topic: 'topic' in summary ? summary.topic : null,
    avatar: summary.avatar,
    join_rule: general
      ? 'Public'
      : summary.room_id === DESIGN
        ? 'Knock'
        : space
          ? 'Restricted'
          : 'Invite',
    can_restrict: !space && summary.room_id !== LAB,
    can_knock: !space,
    history: 'Shared',
    guest_access: false,
    encrypted: !space && !general,
    main_address: general ? '#general:example.org' : null,
    addresses: general ? ['#announcements:example.org'] : [],
    published: general ? true : space ? null : false,
    server: 'example.org',
    version: summary.room_id === LAB ? '10' : '12',
    upgrade_to: summary.room_id === LAB ? '12' : null,
    permissions: listed.map((permission) => ({
      permission,
      level: levels[permission] ?? 50,
      editable: (levels[permission] ?? 50) <= 100,
    })),
    own_level: 100,
    rights: {
      name: true,
      topic: true,
      avatar: true,
      join_rule: true,
      history: true,
      guest_access: true,
      addresses: true,
      encryption: !space,
      permissions: true,
      upgrade: !space,
      space_rooms: space,
    },
  };
}

/** Ada's room list with #lab upgraded: it points to its successor, which she hasn't joined. */
export function upgradedLab(): EngineEvent[] {
  return seedEvents().flatMap((event): EngineEvent[] =>
    event.type === 'RoomsUpdated' && event.data.account === ADA
      ? [
          {
            ...event,
            data: {
              ...event.data,
              list: {
                ...event.data.list,
                rooms: event.data.list.rooms.map((r) =>
                  r.room_id === LAB
                    ? {
                        ...r,
                        successor: {
                          room_id: LAB_V2,
                          reason: 'This room has moved to room version 12.',
                          via: ['example.org'],
                        },
                      }
                    : r,
                ),
              },
            },
          },
        ]
      : [],
  );
}

/** Ada's sessions: this phone, her desktop, and an old laptop nobody verified. */
export function demoSessions(): Session[] {
  const session = (
    id: string,
    name: string,
    daysAgo: number,
    ip: string,
    current: boolean,
    verified: boolean,
  ): Session => ({
    device_id: id,
    name,
    last_seen_ms: DEMO_DAY_MS + 10 * 3_600_000 - daysAgo * 86_400_000,
    last_seen_ip: ip,
    current,
    verified,
  });

  return [
    session('BMPHONE', 'Better Matrix on Android', 0, '203.0.113.7', true, true),
    session('BMDESKTOP', 'Better Matrix on Linux', 1, '198.51.100.23', false, true),
    session('OLDLAPTOP', 'Firefox on Windows', 40, '192.0.2.80', false, false),
  ];
}

/** The threads of #general, as room info lists them. */
export function demoThreads(): ThreadInfo[] {
  return [
    {
      root: DEMO_THREAD.root,
      sender_id: ALAN,
      sender_name: 'Alan Turing',
      body: 'Cross-signing verification passes on Linux, macOS and Windows.',
      replies: 3,
      latest_at: DEMO_DAY_MS + 11 * 3_600_000 + 36 * 60_000,
    },
  ];
}

/** The files shared in #general. */
export function demoFiles(): SharedFile[] {
  const shared = (
    name: string,
    kind: Attachment['kind'],
    [senderId, who]: [string, string],
    minutes: number,
    extra: Partial<Attachment> = {},
  ): SharedFile => ({
    event_id: `$file-${name}`,
    sender_id: senderId,
    sender_name: who,
    timestamp: DEMO_DAY_MS + 10 * 3_600_000 + minutes * 60_000,
    attachment: file(name, kind, extra),
  });

  return [
    shared('release-notes-0.2.pdf', 'File', [ADA, 'Ada Lovelace'], 33),
    shared('screenshot-sidebar.png', 'Image', [GRACE, 'Grace Hopper'], 12, {
      media: DEMO_MEDIA.screenshot,
      dimensions: [1024, 1024],
    }),
    shared('standup.ogg', 'Audio', [KAI, 'Kai'], 2),
  ];
}

/** The emoji a verification asks to compare. */
export function verificationEmojis(): SasEmoji[] {
  return [
    ['🐶', 'Dog'],
    ['🔑', 'Key'],
    ['🎸', 'Guitar'],
    ['🌍', 'Globe'],
    ['🚀', 'Rocket'],
    ['🍕', 'Pizza'],
    ['⚓', 'Anchor'],
  ].map(([symbol, description]) => ({ symbol: symbol!, description: description! }));
}

/**
 * A QR code's look for the demo: finder patterns in three corners, timing lines, and modules
 * from a fixed sequence. It reads as nothing; the demo engine answers scans itself.
 */
export function demoQr(width = 29): QrImage {
  let seed = 20_769;
  const dark: boolean[] = [];

  const finder = (x: number, y: number) => {
    const ring = Math.max(Math.abs(x - 3), Math.abs(y - 3));

    return ring !== 2 && ring !== 4;
  };

  for (let y = 0; y < width; y++) {
    for (let x = 0; x < width; x++) {
      seed = (seed * 48_271) % 2_147_483_647;

      if (x < 8 && y < 8) dark.push(finder(x, y));
      else if (x >= width - 8 && y < 8) dark.push(finder(x - (width - 7), y));
      else if (x < 8 && y >= width - 8) dark.push(finder(x, y - (width - 7)));
      else if (x === 6 || y === 6) dark.push((x + y) % 2 === 0);
      else dark.push(seed % 7 < 3);
    }
  }

  return { width, dark };
}

/** A homeserver that only signs in with passwords, so accounts are created in the app. */
export const PASSWORD_SERVER = 'password.example.org';

/** What the demo's engine reads from any picture: some code's bytes. */
export const DEMO_QR_BYTES = [77, 65, 84, 82, 73, 88, 3, 0];

/** What Ada knows of her co-workers' identities: Grace verified, Alan's changed since. */
export function demoIdentity(userId: string): Identity {
  return { verified: userId === GRACE || userId === ALAN, changed: userId === ALAN };
}

/** Everything the demo starts with: Ada's work and hobby accounts, as the engine reports them. */
export function seedEvents(): EngineEvent[] {
  return [
    { type: 'Restoring', data: [] },
    ...accountEvents(ADA, 'Ada Lovelace', 'example.org', 'Password', work()),
    {
      type: 'LinkPreview',
      data: {
        account: ADA,
        url: RELEASE_NOTES,
        preview: {
          site: 'Better Matrix',
          title: 'Better Matrix 0.2: calls, polls and a member list',
          description:
            'Voice and video calls that work with Element Call, polls in every chat, and a member list with moderation.',
          image: [DEMO_MEDIA.release, [1200, 630]],
        },
      },
    },
    ...accountEvents(ADA_HOBBY, 'ada', 'hobby.chat', 'OAuth', hobby()),
    {
      type: 'ContactDetails',
      data: {
        account: ADA,
        contacts: {
          details: [
            { kind: 'Email', address: 'ada@example.org' },
            { kind: 'Phone', address: '4915112345678' },
          ],
          changeable: true,
        },
      },
    },
    // The hobby server's accounts are managed by an OAuth service, which owns contact details.
    {
      type: 'ContactDetails',
      data: {
        account: ADA_HOBBY,
        contacts: { details: [{ kind: 'Email', address: 'ada@hobby.chat' }], changeable: false },
      },
    },
    {
      type: 'Members',
      data: { request: null, account: ADA, room_id: GENERAL, members: demoMembers() },
    },
    {
      type: 'SecurityChanged',
      data: {
        account: ADA,
        security: { session_verified: true, recovery: 'Enabled', offline_device: 'Off' },
      },
    },
    {
      type: 'SecurityChanged',
      data: {
        account: ADA_HOBBY,
        security: { session_verified: false, recovery: 'Enabled', offline_device: 'Off' },
      },
    },
    { type: 'Sessions', data: { account: ADA, sessions: demoSessions() } },
    { type: 'Threads', data: { account: ADA, room_id: GENERAL, threads: demoThreads() } },
    { type: 'Files', data: { account: ADA, room_id: GENERAL, files: demoFiles() } },
  ];
}

/** A call under way in #general: faces for a voice call, cameras and a shared screen for video. */
export function callInfo(video: boolean): CallInfo {
  const participant = (
    key: string,
    who: Person,
    muted: boolean,
    camera: number | null,
    screen: number | null,
  ) => ({
    key,
    person: who,
    muted,
    camera,
    screen,
  });

  return {
    call: 1,
    account: ADA,
    room_id: GENERAL,
    via: 'Rtc',
    phase: 'Connected',
    video,
    microphone: true,
    camera: video ? 4 : null,
    screen: null,
    participants: [
      participant(
        'grace',
        person(GRACE, 'Grace Hopper'),
        false,
        video ? 1 : null,
        video ? 2 : null,
      ),
      participant('kai', person(KAI, 'Kai Ito'), true, null, null),
      participant('alan', person(ALAN, 'Alan Turing'), false, video ? 3 : null, null),
    ],
  };
}

/** A camera-only call of `people` (the user included): a crowded gallery, or a one-to-one self-view. */
export function callGallery(people: number): CallInfo {
  const info = callInfo(true);

  const participants = info.participants
    .slice(0, Math.max(people - 1, 0))
    .map((p) => ({ ...p, screen: null }));

  while (participants.length + 1 < people) {
    const n = participants.length;
    participants.push({
      key: `guest-${n}`,
      person: person(`@guest-${n}:example.org`, `Guest ${n}`),
      muted: n % 3 === 0,
      camera: n + 10,
      screen: null,
    });
  }

  return { ...info, participants };
}

/** Two shares at once (Grace's and Alan's), for Auto's large share and Grid's comparison. */
export function callShares(): CallInfo {
  const info = callInfo(true);

  const participants = info.participants.map((p, i, all) =>
    i === all.length - 1 ? { ...p, screen: 5 } : p,
  );

  return { ...info, participants };
}

/** A video call ringing from Grace. */
export function ring(): IncomingCall {
  return {
    account: ADA,
    room_id: DM_GRACE,
    caller: person(GRACE, 'Grace Hopper'),
    video: true,
    via: 'Rtc',
  };
}

/** People typing in #general. */
export function typingPeople(): Person[] {
  return [
    person(GRACE, 'Grace Hopper'),
    person(ALAN, 'Alan Turing'),
    person(KAI, 'Kai'),
    person('@mira:example.org', 'Mira'),
  ];
}

/** The widgets pinned to #general: a board, a dashboard and a Jitsi meeting. */
export function demoWidgets(): Widgets {
  const widget = (
    id: string,
    name: string,
    kind: string,
    host: string,
    path: string,
    meeting: Meeting | null = null,
  ): Widget => ({
    id,
    name,
    kind,
    url: `https://${host}${path}`,
    host,
    creator: GRACE,
    can_remove: true,
    meeting,
  });

  return {
    widgets: [
      widget('board', 'Sprint board', 'm.custom', 'board.example.org', '/team'),
      widget('grafana', 'Server dashboard', 'm.grafana', 'grafana.example.org', '/d/1'),
      widget('standup', 'Standup', 'jitsi', 'meet.example.org', '/TeamStandup', 'Jitsi'),
    ],
    can_add: true,
  };
}

/** What the demo's sprint board asks to do when it runs: one of each kind it can name. */
export function demoWidgetPermissions(): WidgetPermission[] {
  return [
    {
      capability: 'org.matrix.msc2762.receive.event:m.room.message',
      kind: 'ReadEvents',
      subject: 'm.room.message',
    },
    {
      capability: 'org.matrix.msc2762.send.event:m.room.message#m.text',
      kind: 'SendEvents',
      subject: 'm.room.message#m.text',
    },
    {
      capability: 'org.matrix.msc2762.receive.state_event:m.room.member',
      kind: 'ReadState',
      subject: 'm.room.member',
    },
    { capability: 'org.matrix.msc4039.download_file', kind: 'DownloadFiles', subject: null },
    { capability: 'io.element.requires_client', kind: 'StayInApp', subject: null },
    { capability: 'town.robin.msc3846.turn_servers', kind: 'Other', subject: null },
  ];
}

/** A message's source, as an encrypted room's event reads decrypted and as it was sent. */
export function demoSource(roomId: string, eventId: string): EventSource {
  const envelope = { sender: ALAN, event_id: eventId, origin_server_ts: DEMO_DAY_MS };

  return {
    room_id: roomId,
    event_id: eventId,
    json: JSON.stringify(
      {
        type: 'm.room.message',
        ...envelope,
        content: { msgtype: 'm.text', body: 'Has anyone tried the new build?' },
      },
      null,
      2,
    ),
    encrypted: JSON.stringify(
      {
        type: 'm.room.encrypted',
        ...envelope,
        content: {
          algorithm: 'm.megolm.v1.aes-sha2',
          ciphertext: 'AwgAEpABqOCzJ2Tp…',
          device_id: 'ALANLAPTOP',
          sender_key: 'Hc1gJ8i3y1cCi0ZmRp3K7XcfuF0pB6wd1Sx4nT2EYfQ',
          session_id: '3yv1Q3wN1cZ9u2lO0d5XG4WeV8m7n2Ck1lZ6aQpTb3Y',
        },
      },
      null,
      2,
    ),
  };
}

/** Room state #general's timeline leaves out, drawn while hidden events show. */
export function hiddenEvents(): EngineEvent[] {
  const hidden = (id: string, kind: string, sender: Person, minutes: number): TimelineEntry => ({
    id,
    kind: {
      type: 'Hidden',
      data: {
        event_id: `$${id}`,
        sender,
        event_type: kind,
        timestamp_ms: DEMO_DAY_MS + 12 * 3_600_000 + minutes * 60_000,
      },
    },
  });

  const grace = { id: GRACE, name: 'Grace Hopper', avatar: null };
  const alan = { id: ALAN, name: 'Alan Turing', avatar: null };

  return [
    {
      type: 'TimelineChanged',
      data: {
        account: ADA,
        key: { room_id: GENERAL, thread: null },
        diffs: [
          { type: 'PushBack', data: hidden('hidden-acl', 'm.room.server_acl', grace, 1) },
          { type: 'PushBack', data: hidden('hidden-widget', 'im.vector.modular.widgets', alan, 2) },
        ],
      },
    },
  ];
}
