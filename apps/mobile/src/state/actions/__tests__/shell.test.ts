import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Command, SidebarLayout } from '@better-matrix/matrix-native/types';

import { ADA, roomSummary, summary } from '../../../test/fixtures';
import { DIRECT } from '../../sections';
import { Store } from '../../store';
import {
  addSection,
  leaveRoom,
  markRoomRead,
  moveRoomToSection,
  setNotificationMode,
  toggleSection,
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
