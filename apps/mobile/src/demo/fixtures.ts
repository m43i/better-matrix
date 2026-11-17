// Sample accounts for demo scenes, ported from the desktop's `apps/desktop/src/demo.rs`: the same
// people, rooms and messages, as engine events. They reach the state through `apply`, like real
// events, so a scene exercises the same path the engine does (ADR 0002).

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  AccountId,
  Attachment,
  CallInfo,
  IncomingCall,
  Invitation,
  MemberRole,
  Members,
  Message,
  MessageContent,
  Person,
  Presence,
  RoomMember,
  RoomPreview,
  RoomSummary,
  SasEmoji,
  Session,
  SharedFile,
  SpaceSummary,
  ThreadInfo,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';

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

export const DM_KAI = '!dm-kai:example.org';

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
} as const;

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
  return { kind, name, size: 1_450_000, media: null, thumbnail: null, dimensions: null, ...extra };
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
  };
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
    topic,
    kind: 'Group',
    unread,
    mentions,
    dm_users: [],
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
    pinned: id === GENERAL ? [DEMO_THREAD.root] : [],
    can_pin: true,
    can_rename: true,
  };
}

function dm(id: string, withId: string, name: string, unread: number): RoomSummary {
  return { ...room(id, name, null, unread, unread), kind: 'Direct', dm_users: [withId] };
}

function space(id: string, name: string, children: string[], members: string[]): SpaceSummary {
  return {
    room_id: id,
    name,
    avatar: null,
    can_add_rooms: true,
    can_rename: true,
    children,
    members,
  };
}

/** How a room begins: created, an invite, and a run of joins that reads as one line. */
function roomEvents(): TimelineEntry[] {
  const ada = person(ADA, 'Ada Lovelace');

  const event = (
    id: string,
    sender: Person,
    change: Extract<TimelineEntry['kind'], { type: 'RoomEvent' }>['data']['change'],
  ): TimelineEntry => ({
    id,
    kind: { type: 'RoomEvent', data: { sender, change } },
  });

  const joined: Array<[string, string]> = [
    [GRACE, 'Grace Hopper'],
    [ALAN, 'Alan Turing'],
    [KAI, 'Kai'],
    ['@mira:example.org', 'Mira'],
    ['@ken:example.org', 'Ken Thompson'],
    ['@barbara:example.org', 'Barbara Liskov'],
  ];

  return [
    event('ev-created', ada, { type: 'Created' }),
    event('ev-invited', ada, { type: 'Invited', data: person(KAI, 'Kai') }),
    ...joined.map(([id, name]) => event(`ev-joined-${id}`, person(id, name), { type: 'Joined' })),
  ];
}

/** A whole timeline: its start, the day's divider, the room's first events, then the messages. */
export function timelineEntries(list: Message[], unread = 0): TimelineEntry[] {
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
    ...roomEvents(),
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

/** Grace's screenshot in #design, an image the demo engine serves. */
export const SCREENSHOT = `$demo-${GRACE}-1`;

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
        room(DESIGN, 'design', 'Mockups, reviews and the component library', 3, 0),
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
      [
        { ...room('!releases:example.org', 'releases', null, 1, 1), history: 'Joined' },
        messages([[ALAN, 'Alan Turing', text('@ada can you sign off 0.2.0?'), 0]]),
      ],
      [
        {
          ...room('!meetup:example.org', 'meetup', 'Monthly Rust meetup', 4, 0),
          notifications: 'MentionsOnly',
          custom_notifications: true,
        },
        [],
      ],
      [room('!lab:example.org', 'lab', 'My own scratch room', 0, 0), []],
      [
        dm(DM_GRACE, GRACE, 'Grace Hopper', 2),
        messages([
          [ADA, 'Ada Lovelace', callCard(true), 0],
          [GRACE, 'Grace Hopper', text('Do you have a minute to look at the sync loop?'), 1],
          [GRACE, 'Grace Hopper', text('It drops the room list after a reconnect.'), 2],
        ]),
      ],
      [{ ...dm('!dm-alan:example.org', ALAN, 'Alan Turing', 0), notifications: 'Mute' }, []],
      // Kai is calling: the card shows the call under way, with the way in.
      [
        { ...dm(DM_KAI, KAI, 'Kai', 1), call: { via: 'Rtc', participants: [KAI] } },
        messages([[KAI, 'Kai', callCard(false), 0]]),
      ],
    ],
    spaces: [
      space(
        WORK_SPACE,
        'Analytical Engines',
        [GENERAL, DESIGN, '!releases:example.org'],
        [ADA, GRACE, ALAN],
      ),
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
      },
    ],
  };
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
        diffs: [{ type: 'Reset', data: timelineEntries(list, summary.unread) }],
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

  return { person: person(id, name), role, presence, verified, level };
}

/** Who is in #general (the member list fills every room the demo engine is asked about). */
export function demoMembers(): Members {
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
  };
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
    who: string,
    minutes: number,
    extra: Partial<Attachment> = {},
  ): SharedFile => ({
    event_id: `$file-${name}`,
    sender_name: who,
    timestamp: DEMO_DAY_MS + 10 * 3_600_000 + minutes * 60_000,
    attachment: file(name, kind, extra),
  });

  return [
    shared('release-notes-0.2.pdf', 'File', 'Ada Lovelace', 33),
    shared('screenshot-sidebar.png', 'Image', 'Grace Hopper', 12, {
      media: DEMO_MEDIA.screenshot,
      dimensions: [1024, 1024],
    }),
    shared('standup.ogg', 'Audio', 'Kai', 2),
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
      data: { account: ADA, security: { session_verified: true, recovery: 'Enabled' } },
    },
    {
      type: 'SecurityChanged',
      data: { account: ADA_HOBBY, security: { session_verified: false, recovery: 'Enabled' } },
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
