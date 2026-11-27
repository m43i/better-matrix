// Demo scenes: named starting states for looking at the app without an account, the mobile
// counterpart of the desktop's `--scene`. Open one in a dev build with
// `bettermatrix://demo/<scene>` (e.g. `adb shell am start -d bettermatrix://demo/room`).
// Each scene is the demo seed plus its own events, a few local changes, and the screens to
// open, in order.

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type { Href } from 'expo-router';

import { routes } from '../routes';
import { setCallLayout, showCallChat } from '../state/actions/calls';
import { setActive, setScope } from '../state/actions/core';
import { updateComposer } from '../state/composer';
import { pathFromUri } from '../state/outgoing';
import { pillAt, userPill } from '../state/pills';
import type { Settings } from '../state/settings';
import { showGiphyKey } from '../state/actions/stickers';
import type { Store } from '../state/store';
import { roomKey, threadKey } from '../state/timeline';
import { widgetLaunches } from '../ui/widgets/launch';
import {
  ADA,
  ADA_HOBBY,
  ALAN,
  batchEvents,
  callGallery,
  callInfo,
  callShares,
  DEMO_BATCH,
  DEMO_THREAD,
  demoPacks,
  DESIGN,
  DESIGN_INVITE,
  DM_GRACE,
  demoQr,
  demoSessions,
  demoWidgets,
  DM_KAI,
  GENERAL,
  GRACE,
  hiddenEvents,
  KAI,
  PASSWORD_SERVER,
  LAB,
  READ_BY_SIX,
  RELEASES,
  ring,
  safetyEvents,
  STRANGER_INVITE,
  SCREENSHOT,
  seedEvents,
  SIGN_OFF,
  STICKER_MESSAGE,
  stickerEvents,
  TEST_PLAN,
  typingPeople,
  upgradedLab,
  verificationEmojis,
  WORK_SPACE,
} from './fixtures';

export interface Scene {
  /** What it shows, for the list of scenes. */
  about: string;
  /** Start from the demo's accounts (default), or signed out. */
  seeded?: boolean;
  /** Settings for this scene, over the defaults. */
  settings?: Partial<Settings>;
  /** Events after the seed. */
  events?: EngineEvent[];
  /** Changes on this device after the events (the account or scope on screen, a draft). */
  setup?: (store: Store) => void;
  /** The screens to open, in order, over the tabs; made as the scene opens when they need to be. */
  routes?: Href[] | (() => Href[]);
}

const general = (store: Store) => setScope(store, ADA, { space: WORK_SPACE });

/** #general with `draft` in its composer, the caret at its end. */
const drafting = (draft: string) => (store: Store) => {
  general(store);
  updateComposer(store, ADA, roomKey(GENERAL), (c) => ({
    ...c,
    draft,
    selection: { start: draft.length, end: draft.length },
  }));
};

/** The app's icon, standing in for a photo picked for sending. */
const PHOTO: number = require('../../assets/images/icon.png');

/**
 * #general's composer holding a photo and a PDF to send. The photo is the bundled icon, copied out
 * of the bundle first as a picked file would be; without it (in tests) the photo has no preview.
 */
const attaching = (store: Store) => {
  general(store);

  const put = (photo: string) =>
    updateComposer(store, ADA, roomKey(GENERAL), (c) => ({
      ...c,
      attachments: [
        { path: pathFromUri(photo), name: 'whiteboard.png', kind: 'image', uri: photo },
        {
          path: '/demo/release-notes.pdf',
          name: 'release-notes.pdf',
          kind: 'file',
          uri: 'file:///demo/release-notes.pdf',
        },
      ],
    }));

  import('expo-asset')
    .then(({ Asset }) => Asset.fromModule(PHOTO).downloadAsync())
    .then((asset) => put(asset.localUri ?? ''))
    .catch(() => put(''));
};

/** Two messages the server holds for #general: one this device wrote, one it can't read. */
const scheduledEvents = (): EngineEvent[] => [
  {
    type: 'ScheduledMessages',
    data: {
      account: ADA,
      messages: [
        {
          delay_id: 'demo-delay-1',
          room_id: GENERAL,
          send_at_ms: Date.now() + 45 * 60_000,
          body: 'Reminder: the 0.2 tag goes out at 15:00.',
          failed: null,
        },
        {
          delay_id: 'demo-delay-2',
          room_id: GENERAL,
          send_at_ms: Date.now() + 26 * 3_600_000,
          body: null,
          failed: null,
        },
        {
          delay_id: 'demo-delay-0',
          room_id: GENERAL,
          send_at_ms: Date.now() - 3_600_000,
          body: 'Office closed on Friday.',
          failed: 'You don’t have permission to post here',
        },
      ],
    },
  },
];

/** Ada's hobby account (an OAuth account) verified, with its sessions: it may link devices. */
function hobbyVerified(): EngineEvent[] {
  return [
    {
      type: 'SecurityChanged',
      data: {
        account: ADA_HOBBY,
        security: { session_verified: true, recovery: 'Enabled', offline_device: 'On' },
      },
    },
    { type: 'Sessions', data: { account: ADA_HOBBY, sessions: demoSessions() } },
  ];
}

export const scenes = {
  home: { about: 'Home: Overview, recent conversations, unread rooms and sections' },
  'search-ranked': {
    about: 'Search: usage favors design and Kai; try g for rooms or a for people',
    events: [
      {
        type: 'SearchUsage',
        data: {
          account: ADA,
          usage: { rooms: { [DESIGN]: 20, [GENERAL]: 2 }, people: { [KAI]: 30 } },
        },
      },
    ],
  },
  'home-light': { about: 'Home in the light appearance', settings: { appearance: 'light' } },
  'home-dark': { about: 'Home in the dark appearance', settings: { appearance: 'dark' } },
  space: { about: 'A space in scope', setup: general },
  dms: { about: 'The direct messages tab', routes: ['/dms'] },
  'account-menu': {
    about: 'The account menu, from the avatar in the header',
    routes: [routes.accountMenu()],
  },
  spaces: { about: 'The space picker, from Home’s title', routes: [routes.spaces()] },
  create: { about: 'The create sheet, from the + in the tab bar', routes: [routes.create()] },
  invitation: {
    about: 'An invitation to accept or decline',
    routes: [routes.invitation(ADA, DESIGN_INVITE)],
  },
  'room-menu': {
    about: 'A room’s long-press menu, with Start a meeting',
    routes: [routes.roomMenu(ADA, GENERAL)],
  },
  room: {
    about: 'The general room: replies, reactions, a poll, a thread, a link card',
    setup: general,
    routes: [routes.room(ADA, GENERAL)],
  },
  'room-bubbles': {
    about: 'The general room in chat bubbles: others at the left, yours at the right',
    settings: { messageLayout: 'bubbles' },
    setup: general,
    routes: [routes.room(ADA, GENERAL)],
  },
  'dm-bubbles': {
    about: 'A direct chat in chat bubbles, with a call card and a voice memo',
    settings: { messageLayout: 'bubbles' },
    routes: [routes.room(ADA, DM_GRACE)],
  },
  design: { about: 'A room with an image', setup: general, routes: [routes.room(ADA, DESIGN)] },
  dm: {
    about: 'A direct chat with a call card and a voice memo',
    routes: [routes.room(ADA, DM_GRACE)],
  },
  'call-card': { about: 'A call under way, with Join', routes: [routes.room(ADA, DM_KAI)] },
  thread: {
    about: 'A thread',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.thread(ADA, DEMO_THREAD.roomId, DEMO_THREAD.root)],
  },
  'message-actions': {
    about: 'A message’s long-press sheet',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.messageActions(ADA, GENERAL, TEST_PLAN)],
  },
  'image-actions': {
    about: 'An image message’s content-aware long-press sheet',
    setup: general,
    routes: [routes.room(ADA, DESIGN), routes.messageActions(ADA, DESIGN, SCREENSHOT)],
  },
  'attachment-actions': {
    about: 'File actions above the full-screen image preview',
    setup: general,
    routes: [
      routes.room(ADA, DESIGN),
      routes.preview(ADA, DESIGN, SCREENSHOT),
      routes.attachmentActions(ADA, DESIGN, SCREENSHOT),
    ],
  },
  'read-receipts': {
    about: 'Who has read a message, from its read indicator',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.readers(ADA, GENERAL, READ_BY_SIX)],
  },
  'pinned-messages': {
    about: 'Shared pinned messages',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.pins(ADA, GENERAL)],
  },
  'saved-messages': {
    about: 'The messages the user saved, from the account menu',
    setup: general,
    routes: [routes.saved(ADA)],
  },
  'forward-message': {
    about: 'Choose a conversation for a forwarded message',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.forward(ADA, GENERAL, TEST_PLAN)],
  },
  react: {
    about: 'Picking a reaction',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.emoji(ADA, GENERAL, { eventId: TEST_PLAN })],
  },
  preview: {
    about: 'An image full screen',
    setup: general,
    routes: [routes.room(ADA, DESIGN), routes.preview(ADA, DESIGN, SCREENSHOT)],
  },
  'preview-gallery': {
    about: 'An image sent in a batch: the gallery’s strip of thumbnails along the bottom',
    setup: general,
    events: batchEvents(),
    routes: [routes.room(ADA, DESIGN), routes.preview(ADA, DESIGN, DEMO_BATCH.second)],
  },
  'preview-audio': {
    about: 'An audio clip of a batch in the preview’s player',
    setup: general,
    events: batchEvents(),
    routes: [routes.room(ADA, DESIGN), routes.preview(ADA, DESIGN, DEMO_BATCH.audio)],
  },
  compose: {
    about: 'A formatted draft in the composer',
    setup: (store) => {
      general(store);
      updateComposer(store, ADA, roomKey(GENERAL), (c) => ({
        ...c,
        draft: '**Release notes** are ready, see the attached changelog.',
      }));
    },
    routes: [routes.room(ADA, GENERAL)],
  },
  'composer-images': {
    about: 'A photo and a file waiting in the composer, the photo as a thumbnail',
    setup: attaching,
    routes: [routes.room(ADA, GENERAL)],
  },
  reply: {
    about: 'Replying to a message',
    setup: (store) => {
      general(store);

      const message = store
        .getState()
        .accounts[0]?.timelines[GENERAL]?.entries.find((e) => e.id === TEST_PLAN);

      if (message?.kind.type === 'Message') {
        const replyTo = message.kind.data;
        updateComposer(store, ADA, roomKey(GENERAL), (c) => ({ ...c, replyTo }));
      }
    },
    routes: [routes.room(ADA, GENERAL)],
  },
  'message-kinds': {
    about: '#releases: a mention, an emote, a place, a spoiler and maths, custom emoji, an edit',
    setup: general,
    routes: [routes.room(ADA, RELEASES)],
  },
  'edit-history': {
    about: 'An edited message’s earlier versions, from its “edited” label',
    setup: general,
    routes: [routes.room(ADA, RELEASES), routes.editHistory(ADA, RELEASES, SIGN_OFF)],
  },
  mention: {
    about: 'Typing @ in the composer: people in the room and @room',
    setup: drafting('Thanks @'),
    routes: [routes.room(ADA, GENERAL)],
  },
  'mention-pill': {
    about: 'A draft with a mention pill, sent as a matrix.to link and m.mentions',
    setup: (store) => {
      drafting('Thanks @Grace Hopper for the review')(store);
      updateComposer(store, ADA, roomKey(GENERAL), (c) => ({
        ...c,
        pills: [pillAt(userPill(GRACE, 'Grace Hopper'), 'Thanks '.length)],
      }));
    },
    routes: [routes.room(ADA, GENERAL)],
  },
  commands: {
    about: 'Typing / in the composer: the slash commands',
    setup: drafting('/'),
    routes: [routes.room(ADA, GENERAL)],
  },
  'emoji-suggestions': {
    about: 'Typing :pa in the composer: custom and standard emoji',
    setup: drafting('Release day :pa'),
    routes: [routes.room(ADA, GENERAL)],
  },
  'emoji-custom': {
    about: 'The composer’s emoji picker with the custom emoji first',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.emoji(ADA, GENERAL)],
  },
  stickers: {
    about: 'The sticker and GIF picker on its stickers, one sent lately first',
    setup: general,
    settings: {
      recentStickers: {
        [ADA]: demoPacks()
          .flatMap((pack) => pack.images)
          .filter((image) => image.shortcode === 'ship'),
      },
    },
    routes: [routes.room(ADA, GENERAL), routes.stickers(ADA, GENERAL)],
  },
  gifs: {
    about: 'The sticker and GIF picker on its GIFs, with a GIPHY key',
    setup: (store) => {
      general(store);
      showGiphyKey(store, 'demo-key');
    },
    routes: [routes.room(ADA, GENERAL), routes.stickers(ADA, GENERAL, undefined, 'gifs')],
  },
  'gifs-key': {
    about: 'The GIFs tab asking for the user’s own GIPHY key',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.stickers(ADA, GENERAL, undefined, 'gifs')],
  },
  packs: {
    about: 'Stickers & emoji: Ada’s pack and #general’s, which she may change',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.packs(ADA, GENERAL)],
  },
  pack: {
    about: 'One of #general’s packs, to rename, add to and use everywhere',
    setup: general,
    routes: [
      routes.room(ADA, GENERAL),
      routes.packs(ADA, GENERAL),
      routes.pack(ADA, `${GENERAL}/`, GENERAL),
    ],
  },
  'pack-image': {
    about: 'A pack’s sticker: short code, description, sticker or emoji',
    setup: general,
    routes: [
      routes.room(ADA, GENERAL),
      routes.pack(ADA, `${GENERAL}/`, GENERAL),
      routes.packImage(ADA, `${GENERAL}/`, 'wave', GENERAL),
    ],
  },
  'new-pack': {
    about: 'Making a pack in #general',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.packs(ADA, GENERAL), routes.newPack(ADA, GENERAL)],
  },
  sticker: {
    about: 'A sticker Grace sent, drawn on its own',
    events: stickerEvents(),
    setup: general,
    routes: [routes.room(ADA, GENERAL)],
  },
  'sticker-actions': {
    about: 'A sticker’s long-press sheet, with Add to my stickers',
    events: stickerEvents(),
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.messageActions(ADA, GENERAL, STICKER_MESSAGE)],
  },
  'location-map': {
    about: 'A shared place on its map',
    setup: general,
    routes: [routes.room(ADA, RELEASES), routes.locationMap(ADA, RELEASES, `$demo-${GRACE}-2`)],
  },
  location: {
    about: 'Sharing a place: the phone’s position or one found by name',
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.location(ADA, GENERAL)],
  },
  schedule: {
    about: 'Scheduling the draft, from a long press on Send',
    setup: drafting('Reminder: the 0.2 tag goes out at 15:00.'),
    routes: [routes.room(ADA, GENERAL), routes.schedule(ADA, GENERAL)],
  },
  'scheduled-bar': {
    about: 'The bar above the composer saying what is scheduled here',
    setup: general,
    events: scheduledEvents(),
    routes: [routes.room(ADA, GENERAL)],
  },
  scheduled: {
    about: 'A conversation’s scheduled messages, to send now or cancel',
    setup: general,
    events: scheduledEvents(),
    routes: [routes.room(ADA, GENERAL), routes.scheduled(ADA, GENERAL)],
  },
  typing: {
    about: 'People typing',
    setup: general,
    events: [{ type: 'Typing', data: { account: ADA, room_id: GENERAL, people: typingPeople() } }],
    routes: [routes.room(ADA, GENERAL)],
  },
  'poll-form': {
    about: 'A new poll',
    routes: [routes.room(ADA, GENERAL), routes.newPoll(ADA, GENERAL)],
  },
  'room-info': {
    about: 'Room details and members',
    routes: [routes.room(ADA, GENERAL), routes.roomInfo(ADA, GENERAL)],
  },
  'room-log': {
    about: 'A room’s log: its changes, newest first, from room info',
    setup: general,
    routes: [
      routes.room(ADA, GENERAL),
      routes.roomInfo(ADA, GENERAL),
      routes.roomLog(ADA, GENERAL),
    ],
  },
  'room-changes': {
    about: 'Room changes shown in #general’s conversation',
    settings: { showRoomChanges: true },
    setup: general,
    routes: [routes.room(ADA, GENERAL)],
  },
  widgets: {
    about: 'A room’s widgets in one list, its meeting first; the header’s + holds the rest',
    routes: [routes.roomInfo(ADA, GENERAL), routes.widgets(ADA, GENERAL)],
  },
  meeting: {
    about: 'A room with pinned Jitsi, Teams and Zoom meetings in the video chooser',
    setup: general,
    events: [{ type: 'Widgets', data: { account: ADA, room_id: GENERAL, widgets: demoWidgets() } }],
    routes: [routes.room(ADA, GENERAL)],
  },
  'meeting-start': {
    about: 'A room where a meeting can start, none pinned: its video button offers to start one',
    setup: general,
    events: [
      {
        type: 'Widgets',
        data: { account: ADA, room_id: DESIGN, widgets: { widgets: [], can_add: true } },
      },
    ],
    routes: [routes.room(ADA, DESIGN)],
  },
  'meeting-command': {
    about: 'Typing /mee in a room that can start a meeting: /meet suggested',
    setup: drafting('/mee'),
    routes: [routes.room(ADA, GENERAL)],
  },
  'widget-permissions': {
    about: 'A widget running in the app, asking for permissions over its page',
    setup: general,
    events: [{ type: 'Widgets', data: { account: ADA, room_id: GENERAL, widgets: demoWidgets() } }],
    // The app's own launch, as a tap on the widget registers it.
    routes: () => [
      routes.room(ADA, GENERAL),
      routes.widget(ADA, GENERAL, { type: 'widget', widgetId: 'board' }, widgetLaunches.register()),
    ],
  },
  integrations: {
    about: 'The integration manager’s terms, before it opens',
    // The app's own launch, as Bots and integrations registers it.
    routes: () => [
      routes.widgets(ADA, GENERAL),
      routes.widget(ADA, GENERAL, { type: 'integrations' }, widgetLaunches.register()),
    ],
  },
  'event-source': {
    about: 'A message’s source, decrypted and as sent',
    routes: [routes.room(ADA, GENERAL), routes.eventSource(ADA, GENERAL, TEST_PLAN)],
  },
  'hidden-events': {
    about: 'Hidden events drawn in #general (developer tools)',
    settings: { showHiddenEvents: true },
    setup: general,
    events: hiddenEvents(),
    routes: [routes.room(ADA, GENERAL)],
  },
  member: {
    about: 'Someone in the member list, verified',
    routes: [routes.roomInfo(ADA, GENERAL), routes.member(ADA, GENERAL, GRACE)],
  },
  'member-unverified': {
    about: 'A member nobody verified yet, with Verify',
    routes: [routes.roomInfo(ADA, GENERAL), routes.member(ADA, GENERAL, KAI)],
  },
  'member-changed': {
    about: 'A verified member whose identity changed since',
    routes: [routes.roomInfo(ADA, GENERAL), routes.member(ADA, GENERAL, ALAN)],
  },
  invite: { about: 'Inviting people', routes: [routes.invite(ADA, GENERAL)] },
  'new-chat': { about: 'Starting a direct chat', routes: [routes.newChat()] },
  'new-room': { about: 'Making a room', routes: [routes.newRoom(WORK_SPACE)] },
  'new-space': { about: 'Making a space', routes: [routes.newSpace()] },
  'new-room-public': {
    about: 'Making a public room in a space: its address, directory listing and encryption',
    routes: [routes.newRoom(WORK_SPACE, 'Public')],
  },
  'new-space-public': {
    about: 'Making a public space with an address',
    routes: [routes.newSpace('Public')],
  },
  'room-settings': {
    about: 'A public room’s settings, every part an admin may change',
    routes: [routes.roomInfo(ADA, GENERAL), routes.roomSettings(ADA, GENERAL)],
  },
  'room-settings-upgrade': {
    about: 'A private room’s settings, with encryption on and an upgrade offered',
    routes: [routes.roomSettings(ADA, LAB)],
  },
  'room-permissions': {
    about: 'A room’s permission levels, one above Ada’s own',
    routes: [routes.roomSettings(ADA, GENERAL), routes.roomPermissions(ADA, GENERAL)],
  },
  'space-settings': {
    about: 'A space’s settings',
    routes: [routes.roomSettings(ADA, WORK_SPACE)],
  },
  'member-requests': {
    about: 'Requests to join and someone banned, in #design’s member list',
    routes: [routes.room(ADA, DESIGN), routes.roomInfo(ADA, DESIGN)],
  },
  tombstone: {
    about: 'An upgraded room: the banner to its successor instead of the composer',
    events: upgradedLab(),
    routes: [routes.room(ADA, LAB)],
  },
  'space-menu': {
    about: 'A space’s sheet, from a long press in the space picker',
    routes: [routes.spaceMenu(ADA, WORK_SPACE)],
  },
  'space-rooms': {
    about: 'Arranging a space’s rooms',
    setup: general,
    routes: [routes.spaceRooms(ADA, WORK_SPACE)],
  },
  'leave-space': {
    about: 'Leaving a space, choosing which of its rooms to leave too',
    routes: [routes.leaveSpace(ADA, WORK_SPACE)],
  },
  'status-message': {
    about: 'Editing a status message with a calendar button for expiry',
    routes: [routes.accountMenu(), routes.statusMessage(ADA)],
  },
  settings: { about: 'Settings', routes: [routes.settings()] },
  'settings-giphy': {
    about: 'Messages & media: entering the user’s own GIPHY key',
    routes: [routes.settings(), routes.settingsMessagesMedia(), routes.settingsGiphy()],
  },
  'settings-giphy-saved': {
    about: 'Messages & media: a GIPHY key saved, offered only to replace or remove',
    setup: (store) => showGiphyKey(store, 'demo-key'),
    routes: [routes.settings(), routes.settingsMessagesMedia(), routes.settingsGiphy()],
  },
  'settings-preferences': {
    about: 'General settings: language, display name order and developer tools',
    setup: (store) =>
      store.update((s) => ({
        ...s,
        accounts: s.accounts.map((account) =>
          account.summary.id === ADA
            ? { ...account, summary: { ...account.summary, display_name: 'Lovelace, Ada' } }
            : account,
        ),
      })),
    routes: [routes.settings(), routes.settingsGeneral()],
  },
  'settings-appearance': {
    about: 'Theme and message layout',
    routes: [routes.settings(), routes.settingsAppearance()],
  },
  'settings-messages-media': {
    about: 'Link previews, GIFs, maps and room changes',
    routes: [routes.settings(), routes.settingsMessagesMedia()],
  },
  'settings-notifications': {
    about: 'Notification defaults',
    routes: [routes.settings(), routes.settingsNotifications()],
  },
  'settings-profile': {
    about: 'Profile and contact details',
    routes: [routes.settings(), routes.settingsProfile(ADA)],
  },
  'settings-account': {
    about: 'Account and security',
    routes: [routes.settings(), routes.settingsAccount(ADA)],
  },
  'settings-sessions': {
    about: 'Sessions',
    routes: [routes.settings(), routes.settingsSessions(ADA)],
  },
  'settings-safety': {
    about:
      'Safety: media in private rooms, a hidden invitation from a stranger, an ignored spammer',
    events: safetyEvents(),
    routes: [routes.settings(), routes.settingsSafety(ADA)],
  },
  'hidden-invitation': {
    about: 'A stranger’s hidden invitation, opened from Settings → Safety, with Ignore and Report',
    events: safetyEvents(),
    routes: [routes.settingsSafety(ADA), routes.invitation(ADA, STRANGER_INVITE)],
  },
  'hidden-media': {
    about: 'The public design room with media in private rooms only: the image waits for Show',
    events: safetyEvents(),
    setup: general,
    routes: [routes.room(ADA, DESIGN)],
  },
  report: {
    about: 'Reporting a message, with a reason and Also ignore',
    setup: general,
    routes: [
      routes.room(ADA, GENERAL),
      routes.report(
        ADA,
        { type: 'Message', data: { room_id: GENERAL, event_id: TEST_PLAN } },
        'Alan Turing',
        ALAN,
      ),
    ],
  },
  'settings-sessions-oauth': {
    about: 'Sessions of a verified OAuth account, with Link a new device',
    events: hobbyVerified(),
    setup: (store) => setActive(store, ADA_HOBBY),
    routes: [routes.settings(), routes.settingsSessions(ADA_HOBBY)],
  },
  'export-keys': {
    about: 'Exporting message keys with a passphrase',
    routes: [routes.settings(), routes.settingsAccount(ADA), routes.settingsExportKeys(ADA)],
  },
  'change-password': {
    about: 'Changing a password account’s password',
    routes: [routes.settings(), routes.settingsAccount(ADA), routes.settingsPassword(ADA)],
  },
  'link-device': {
    about: 'Linking a new device: this session’s QR code',
    events: hobbyVerified(),
    setup: (store) => setActive(store, ADA_HOBBY),
    routes: [routes.linkDevice(ADA_HOBBY)],
  },
  'link-device-scan': {
    about: 'Linking a new device by scanning its code (tap the placeholder to scan)',
    events: hobbyVerified(),
    setup: (store) => setActive(store, ADA_HOBBY),
    routes: [routes.linkDevice(ADA_HOBBY, true)],
  },
  'settings-info': {
    about: 'Version and licences',
    routes: [routes.settings(), routes.settingsInfo()],
  },
  unverified: {
    about: 'An unverified session, with its banner',
    setup: (store) => setActive(store, ADA_HOBBY),
  },
  'unverified-account': {
    about: 'Account and security for an unverified session',
    setup: (store) => setActive(store, ADA_HOBBY),
    routes: [routes.settings(), routes.settingsAccount(ADA_HOBBY)],
  },
  verify: {
    about: 'Comparing emoji to verify a session',
    events: [
      {
        type: 'Verification',
        data: {
          account: ADA,
          flow_id: 'demo',
          step: { type: 'Compare', data: verificationEmojis() },
        },
      },
    ],
    routes: [routes.verify(ADA, 'demo')],
  },
  'verify-qr': {
    about: 'Verifying with a QR code: this side’s code, Scan their code, or emoji',
    events: [
      {
        type: 'Verification',
        data: {
          account: ADA,
          flow_id: 'demo-qr',
          step: { type: 'Ready', data: { qr: demoQr(), scan: true } },
        },
      },
    ],
    routes: [routes.verify(ADA, 'demo-qr')],
  },
  'verify-scanned': {
    about: 'The other side scanned this side’s code: did it say so?',
    events: [
      {
        type: 'Verification',
        data: { account: ADA, flow_id: 'demo-scanned', step: { type: 'Scanned' } },
      },
    ],
    routes: [routes.verify(ADA, 'demo-scanned')],
  },
  'verify-person': {
    about: 'Verifying another person from their profile',
    routes: [routes.verifyUser(ADA, KAI)],
  },
  'verify-person-incoming': {
    about: 'Another person asks to verify',
    events: [
      {
        type: 'Verification',
        data: {
          account: ADA,
          flow_id: 'demo-grace',
          step: {
            type: 'Incoming',
            data: {
              other_device: 'GRACEPHONE',
              user: { id: GRACE, name: 'Grace Hopper', avatar: null },
            },
          },
        },
      },
    ],
  },
  'verify-incoming': {
    about: 'Another session asks to verify',
    events: [
      {
        type: 'Verification',
        data: {
          account: ADA,
          flow_id: 'demo-in',
          step: { type: 'Incoming', data: { other_device: 'BMDESKTOP', user: null } },
        },
      },
    ],
  },
  recovery: { about: 'Entering the recovery key', routes: [routes.recovery(ADA)] },
  ended: {
    about: 'A session the server ended',
    events: [{ type: 'SessionEnded', data: { account: ADA_HOBBY, kept: true } }],
  },
  'sign-in': { about: 'Signing in, signed out', seeded: false },
  'add-account': { about: 'Adding another account', routes: [routes.signIn()] },
  'sign-in-qr': {
    about: 'Signing in by scanning a signed-in device’s code (tap the placeholder to scan)',
    routes: [routes.signInWithQr()],
  },
  register: {
    about:
      'Creating an account on a password server; Create an account asks for the terms, a token, then an email address',
    routes: [routes.register(PASSWORD_SERVER)],
  },
  'call-voice': {
    about: 'A voice call',
    events: [
      { type: 'CallUpdated', data: callInfo(false) },
      {
        type: 'CallLevels',
        data: [
          ['grace', 190],
          ['', 40],
        ],
      },
    ],
    routes: [routes.call()],
  },
  'call-video': {
    about: 'A video call',
    events: [{ type: 'CallUpdated', data: callInfo(true) }],
    routes: [routes.call()],
  },
  'call-bar': {
    about: 'A call running behind Home',
    events: [{ type: 'CallUpdated', data: callInfo(false) }],
  },
  'call-gallery': {
    about: 'A video call of six in a gallery',
    events: [{ type: 'CallUpdated', data: callGallery(6) }],
    routes: [routes.call()],
  },
  'call-many': {
    about: 'A crowded video call that pages',
    events: [{ type: 'CallUpdated', data: callGallery(30) }],
    routes: [routes.call()],
  },
  'call-direct': {
    about: 'A one-to-one video call with a small self-view',
    events: [{ type: 'CallUpdated', data: callGallery(2) }],
    routes: [routes.call()],
  },
  'call-shares': {
    about: 'Two shared screens, the participant strip below',
    events: [{ type: 'CallUpdated', data: callShares() }],
    routes: [routes.call()],
  },
  'call-focus': {
    about: 'Focus on the speaker, the others in the participant strip',
    events: [
      { type: 'CallUpdated', data: callGallery(6) },
      {
        type: 'CallLevels',
        data: [
          ['grace', 190],
          ['', 40],
        ],
      },
    ],
    setup: (store) => setCallLayout(store, { type: 'Focus', pin: null }),
    routes: [routes.call()],
  },
  'call-chat': {
    about: 'The general room’s chat under its video call’s compact stage',
    events: [{ type: 'CallUpdated', data: callInfo(true) }],
    setup: (store) => {
      general(store);
      showCallChat(store, true);
    },
    routes: [routes.room(ADA, GENERAL)],
  },
  'call-voice-chat': {
    about: 'The general room’s chat under its voice call’s faces',
    events: [
      { type: 'CallUpdated', data: callInfo(false) },
      {
        type: 'CallLevels',
        data: [
          ['grace', 190],
          ['', 40],
        ],
      },
    ],
    setup: (store) => {
      general(store);
      showCallChat(store, true);
    },
    routes: [routes.room(ADA, GENERAL)],
  },
  ringing: { about: 'A call ringing', events: [{ type: 'IncomingCall', data: ring() }] },
  'link-preview': {
    about: 'A link to a room nobody joined: its preview with Join',
    routes: [routes.link('https://matrix.to/#/%23rust%3Amatrix.org?via=matrix.org')],
  },
  'link-joined': {
    about: 'A link to a joined room by its alias: it opens the room',
    routes: [routes.link('matrix:r/general:example.org')],
  },
  'link-user': {
    about: 'A link to a person: their profile with Start chat',
    routes: [routes.link(`https://matrix.to/#/${GRACE}`)],
  },
  de: { about: 'Home in German', settings: { language: 'de' } },
  'fr-settings': {
    about: 'Settings in French',
    settings: { language: 'fr' },
    routes: [routes.settings()],
  },
  'thread-de': {
    about: 'A thread in German',
    settings: { language: 'de' },
    setup: general,
    routes: [routes.room(ADA, GENERAL), routes.thread(ADA, DEMO_THREAD.roomId, DEMO_THREAD.root)],
  },
} satisfies Record<string, Scene>;

export type SceneName = keyof typeof scenes;

export function isScene(name: string): name is SceneName {
  return Object.hasOwn(scenes, name);
}

/** The events that set a scene up: the seed (unless signed out), then the scene's own. */
export function sceneEvents(scene: Scene): EngineEvent[] {
  return [
    ...(scene.seeded === false ? [{ type: 'Restoring' as const, data: [] }] : seedEvents()),
    ...(scene.events ?? []),
  ];
}

/** The thread key a scene's thread uses, for tests. */
export const demoThreadKey = threadKey(DEMO_THREAD.roomId, DEMO_THREAD.root);
