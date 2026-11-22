import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Command, SidebarLayout } from '@better-matrix/matrix-native/types';

import { t } from '../../../i18n';
import { ADA, roomSummary, summary } from '../../../test/fixtures';
import { DIRECT, FAVOURITES } from '../../sections';
import { Store } from '../../store';
import type { AccountData } from '../../types';
import {
  addSection,
  leaveRoom,
  markRoomRead,
  moveRoomToSection,
  setNotificationMode,
  setRoomPriority,
  setUnread,
  toggleSection,
  unmarkOpened,
} from '../shell';

const LAYOUT: SidebarLayout = { scopes: {}, changed_at: 10 };

function isCommand<T extends Command['type']>(
  command: Command,
  type: T,
): command is Extract<Command, { type: T }> {
  return command.type === type;
}

function setUp({ layoutLoaded = true } = {}) {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  engine.emit({
    type: 'RoomsUpdated',
    data: {
      account: ADA,
      list: {
        rooms: [roomSummary('!a'), roomSummary('!dm', 'Grace', 'Direct')],
        spaces: [],
        invitations: [],
      },
    },
  });

  if (layoutLoaded) engine.emit({ type: 'SidebarLayout', data: { account: ADA, layout: LAYOUT } });

  const finish = (request: number, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error ? { Err: error } : { Ok: null } },
    });

  return { store, sent, finish, account: () => store.getState().accounts[0]! };
}

/** Lets the promises answering a request settle. */
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

const ofType = <T extends Command['type']>(sent: Array<{ command: Command }>, type: T) =>
  sent.reduce<Extract<Command, { type: T }>[]>((commands, { command }) => {
    if (isCommand(command, type)) commands.push(command);

    return commands;
  }, []);

describe('layout actions', () => {
  it('collapsing a section should save the layout to the account', () => {
    const { store, sent } = setUp();
    toggleSection(store, ADA, 'home', DIRECT);
    expect(ofType(sent, 'SetSidebarLayout')[0]!.data.layout.scopes.home!.collapsed).toEqual([
      DIRECT,
    ]);
  });

  it('a change should show before the server has it', () => {
    const { store, account } = setUp();
    toggleSection(store, ADA, 'home', DIRECT);
    expect(account().sidebar.scopes.home!.collapsed).toEqual([DIRECT]);
  });

  it('a change should be later than the layout it changes', () => {
    const { store, account } = setUp();
    toggleSection(store, ADA, 'home', DIRECT);
    expect(account().sidebar.changed_at).toBeGreaterThan(LAYOUT.changed_at);
  });

  it('a change before the saved layout arrived should be refused with a notice', () => {
    const { store, sent } = setUp({ layoutLoaded: false });
    toggleSection(store, ADA, 'home', DIRECT);
    expect([ofType(sent, 'SetSidebarLayout').length, store.getState().notices.length]).toEqual([
      0, 1,
    ]);
  });

  it('changes made while a save runs should wait for it', () => {
    const { store, sent } = setUp();
    toggleSection(store, ADA, 'home', DIRECT);
    toggleSection(store, ADA, 'home', 'rooms');
    expect(ofType(sent, 'SetSidebarLayout')).toHaveLength(1);
  });

  it('changes made while a save runs should be saved once it finished', async () => {
    const { store, sent, finish } = setUp();
    toggleSection(store, ADA, 'home', DIRECT);
    toggleSection(store, ADA, 'home', 'rooms');
    finish(sent[0]!.request);
    await settled();
    expect(ofType(sent, 'SetSidebarLayout').at(-1)!.data.layout.scopes.home!.collapsed).toEqual([
      DIRECT,
      'rooms',
    ]);
  });

  it('a failed save should tell the user', async () => {
    const { store, sent, finish } = setUp();
    toggleSection(store, ADA, 'home', DIRECT);
    finish(sent[0]!.request, 'offline');
    await settled();
    expect(store.getState().notices.at(-1)!.message).toBe('Could not save the sidebar: offline');
  });

  it('a new section made from a room should hold that room', () => {
    const { store, account } = setUp();
    addSection(store, ADA, 'home', 'Team', '!a');
    expect(account().sidebar.scopes.home!.sections).toEqual([
      { id: 'section-1', name: 'Team', rooms: ['!a'] },
    ]);
  });

  it('a section without a name should not be made', () => {
    const { store, sent } = setUp();
    addSection(store, ADA, 'home', '   ');
    expect(ofType(sent, 'SetSidebarLayout')).toHaveLength(0);
  });

  it('moving a room to its built-in section should take it out of the user sections', () => {
    const { store, account } = setUp();
    addSection(store, ADA, 'home', 'Team', '!dm');
    moveRoomToSection(store, ADA, 'home', '!dm', DIRECT);
    expect(account().sidebar.scopes.home!.sections[0]!.rooms).toEqual([]);
  });
});

describe('room actions', () => {
  it('choosing a notification mode should tell the engine', () => {
    const { store, sent } = setUp();
    setNotificationMode(store, ADA, '!a', 'Mute');
    expect(ofType(sent, 'SetNotificationMode')[0]!.data).toEqual({
      account: ADA,
      room_id: '!a',
      mode: 'Mute',
    });
  });

  it('choosing a notification mode should show at once', () => {
    const { store, account } = setUp();
    setNotificationMode(store, ADA, '!a', 'MentionsOnly');
    const room = account().rooms.find((r) => r.room_id === '!a')!;
    expect([room.notifications, room.custom_notifications]).toEqual(['MentionsOnly', true]);
  });

  it('following the default should take the account default mode', () => {
    const { store, account } = setUp();
    setNotificationMode(store, ADA, '!a', 'Mute');
    setNotificationMode(store, ADA, '!a', null);
    const room = account().rooms.find((r) => r.room_id === '!a')!;
    expect([room.notifications, room.custom_notifications]).toEqual(['AllMessages', false]);
  });

  it('marking a room read should open its main timeline, mark it and close it again', async () => {
    const { store, sent, finish } = setUp();
    const done = markRoomRead(store, ADA, '!a');
    expect(ofType(sent, 'VisitRoom')).toEqual([]);
    finish(sent.find((s) => s.command.type === 'OpenTimeline')!.request);
    await settled();
    expect(ofType(sent, 'MarkRead')[0]!.data).toMatchObject({
      key: { room_id: '!a', thread: null },
      event_id: null,
      fully_read: true,
    });
    finish(sent.find((s) => s.command.type === 'MarkRead')!.request);
    await done;
    expect(sent.map((s) => s.command.type).slice(-1)).toEqual(['CloseTimeline']);
  });

  it('a room whose timeline did not open should not be marked', async () => {
    const { store, sent, finish } = setUp();
    const done = markRoomRead(store, ADA, '!a');
    finish(sent.find((s) => s.command.type === 'OpenTimeline')!.request, 'store locked');
    await done;
    expect(ofType(sent, 'MarkRead')).toEqual([]);
  });

  it('a marked room whose timeline did not open should stay marked', async () => {
    const { store, sent, finish, account } = setUp();
    setUnread(store, ADA, '!a', true);
    const done = markRoomRead(store, ADA, '!a');
    finish(sent.find((s) => s.command.type === 'OpenTimeline')!.request, 'store locked');
    await done;
    expect(account().rooms.find((r) => r.room_id === '!a')!.marked_unread).toBe(true);
  });

  it('a marked room read through its timeline should lose the mark', async () => {
    const { store, sent, finish, account } = setUp();
    setUnread(store, ADA, '!a', true);
    const done = markRoomRead(store, ADA, '!a');
    finish(sent.find((s) => s.command.type === 'OpenTimeline')!.request);
    await settled();
    finish(sent.find((s) => s.command.type === 'MarkRead')!.request);
    await done;
    expect(account().rooms.find((r) => r.room_id === '!a')!.marked_unread).toBe(false);
  });

  it('deleting a direct chat should also forget it', () => {
    const { store, sent } = setUp();
    leaveRoom(store, ADA, '!dm', true);
    expect(ofType(sent, 'LeaveRoom')[0]!.data).toEqual({
      account: ADA,
      room_id: '!dm',
      forget: true,
    });
  });
});

describe('organising rooms', () => {
  const roomOf = (account: () => AccountData, id: string) =>
    account().rooms.find((r) => r.room_id === id)!;

  it('adding a room to the favourites should show at once and tell the engine', () => {
    const { store, sent, account } = setUp();
    setRoomPriority(store, ADA, '!a', 'Favourite');
    expect(roomOf(account, '!a').priority).toBe('Favourite');
    expect(ofType(sent, 'SetRoomPriority')[0]!.data).toEqual({
      account: ADA,
      room_id: '!a',
      priority: 'Favourite',
    });
  });

  it('choosing where a room already is should send nothing', () => {
    const { store, sent } = setUp();
    setRoomPriority(store, ADA, '!a', 'Normal');
    expect(ofType(sent, 'SetRoomPriority')).toEqual([]);
  });

  it('a failed move should tell the user', async () => {
    const { store, sent, finish } = setUp();
    setRoomPriority(store, ADA, '!a', 'Low');
    finish(sent.at(-1)!.request, 'offline');
    await settled();
    expect(store.getState().notices.at(-1)!.message).toBe(
      t('notice.room_mark', { error: 'offline' }),
    );
  });

  it('moving a room into the favourites section should make it a favourite', () => {
    const { store, sent } = setUp();
    moveRoomToSection(store, ADA, 'home', '!a', FAVOURITES);
    expect(ofType(sent, 'SetRoomPriority')[0]!.data.priority).toBe('Favourite');
  });

  it('moving a favourite into a section of the user should take its tag away', () => {
    const { store, sent } = setUp();
    setRoomPriority(store, ADA, '!a', 'Favourite');
    addSection(store, ADA, 'home', 'Team');
    moveRoomToSection(store, ADA, 'home', '!a', 'section-1');
    expect(ofType(sent, 'SetRoomPriority').at(-1)!.data.priority).toBe('Normal');
  });

  it('marking a room unread should show at once and tell the engine', () => {
    const { store, sent, account } = setUp();
    setUnread(store, ADA, '!a', true);
    expect(roomOf(account, '!a').marked_unread).toBe(true);
    expect(ofType(sent, 'SetUnread')[0]!.data).toEqual({
      account: ADA,
      room_id: '!a',
      unread: true,
    });
  });

  it('taking the unread mark away should clear only the mark', () => {
    const { store, account } = setUp();
    const before = roomOf(account, '!a');
    setUnread(store, ADA, '!a', true);
    setUnread(store, ADA, '!a', false);
    const room = roomOf(account, '!a');
    expect([room.marked_unread, room.unread, room.mentions]).toEqual([
      false,
      before.unread,
      before.mentions,
    ]);
  });

  it('a failed move should show the room where it was', async () => {
    const { store, sent, finish, account } = setUp();
    setRoomPriority(store, ADA, '!a', 'Low');
    finish(sent.at(-1)!.request, 'offline');
    await settled();
    expect(roomOf(account, '!a').priority).toBe('Normal');
  });

  it('a favourite should keep its tag while the layout that would take the move still loads', () => {
    const { store, sent } = setUp({ layoutLoaded: false });
    setRoomPriority(store, ADA, '!a', 'Favourite');
    addSection(store, ADA, 'home', 'Team', '!a');
    moveRoomToSection(store, ADA, 'home', '!a', 'rooms');
    expect(ofType(sent, 'SetRoomPriority').map((c) => c.data.priority)).toEqual(['Favourite']);
  });

  it('moving a room into the favourites should keep its place in a section of the user', () => {
    const { store, account } = setUp();
    addSection(store, ADA, 'home', 'Team', '!a');
    moveRoomToSection(store, ADA, 'home', '!a', FAVOURITES);
    expect(account().sidebar.scopes['home']!.sections[0]!.rooms).toEqual(['!a']);
  });

  it('naming a new section for a favourite should take its tag away', () => {
    const { store, sent } = setUp();
    setRoomPriority(store, ADA, '!a', 'Favourite');
    addSection(store, ADA, 'home', 'Team', '!a');
    expect(ofType(sent, 'SetRoomPriority').at(-1)!.data.priority).toBe('Normal');
  });

  it('opening a room marked unread should take the mark away', () => {
    const { store, sent, account } = setUp();
    setUnread(store, ADA, '!a', true);
    unmarkOpened(store, ADA, '!a');
    expect(roomOf(account, '!a').marked_unread).toBe(false);
    expect(ofType(sent, 'SetUnread').at(-1)!.data.unread).toBe(false);
  });

  it('opening a room not marked unread should send nothing', () => {
    const { store, sent } = setUp();
    unmarkOpened(store, ADA, '!a');
    expect(ofType(sent, 'SetUnread')).toEqual([]);
  });
});
