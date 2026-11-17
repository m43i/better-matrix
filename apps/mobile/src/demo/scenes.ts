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
import type { Settings } from '../state/settings';
import type { Store } from '../state/store';
import { roomKey, threadKey } from '../state/timeline';
import {
  ADA,
  ADA_HOBBY,
  callGallery,
  callInfo,
  callShares,
  DEMO_THREAD,
  DESIGN,
  DESIGN_INVITE,
  DM_GRACE,
  DM_KAI,
  GENERAL,
  GRACE,
  KAI,
  READ_BY_SIX,
  ring,
  SCREENSHOT,
  seedEvents,
  TEST_PLAN,
  typingPeople,
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
  /** The screens to open, in order, over the tabs. */
  routes?: Href[];
}

const general = (store: Store) => setScope(store, ADA, { space: WORK_SPACE });

export const scenes = {
  home: { about: 'Home: sections, unread rooms, direct messages' },
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
  'room-menu': { about: 'A room’s long-press menu', routes: [routes.roomMenu(ADA, GENERAL)] },
  room: {
    about: 'The general room: replies, reactions, a poll, a thread, a link card',
    setup: general,
    routes: [routes.room(ADA, GENERAL)],
  },
  design: { about: 'A room with an image', setup: general, routes: [routes.room(ADA, DESIGN)] },
  dm: { about: 'A direct chat with a call card', routes: [routes.room(ADA, DM_GRACE)] },
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
  member: {
    about: 'Someone in the member list',
    routes: [routes.roomInfo(ADA, GENERAL), routes.member(ADA, GENERAL, GRACE)],
  },
  invite: { about: 'Inviting people', routes: [routes.invite(ADA, GENERAL)] },
  'new-chat': { about: 'Starting a direct chat', routes: [routes.newChat()] },
  'new-room': { about: 'Making a room', routes: [routes.newRoom(WORK_SPACE)] },
  'new-space': { about: 'Making a space', routes: [routes.newSpace()] },
  settings: { about: 'Settings', routes: [routes.settings()] },
  'settings-preferences': {
    about: 'Language and appearance',
    routes: [routes.settings(), routes.settingsPreferences()],
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
  'verify-incoming': {
    about: 'Another session asks to verify',
    events: [
      {
        type: 'Verification',
        data: {
          account: ADA,
          flow_id: 'demo-in',
          step: { type: 'Incoming', data: { other_device: 'BMDESKTOP' } },
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
