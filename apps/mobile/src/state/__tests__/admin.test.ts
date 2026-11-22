import {
  addressInvalid,
  addressLocalPart,
  addressToAdd,
  allAddresses,
  choicesOffered,
  createRoomCommand,
  EMPTY_NEW_ROOM,
  encryptsByDefault,
  joinedChildren,
  joinRuleParam,
  joinRulesForSettings,
  joinRulesOffered,
  leaveSpaceCommand,
  levelChoice,
  type NewRoomDraft,
  parseLevel,
  permissionEditable,
  permissionSetting,
  reorder,
  roomsToAdd,
  serverOf,
  settingsParts,
  spaceChildren,
  successorRoute,
  textSetting,
  withPending,
} from '../admin';
import { ADA, rights, roomSettings, roomSummary, spaceSummary } from '../../test/fixtures';

const draft = (change: Partial<NewRoomDraft>): NewRoomDraft => ({
  ...EMPTY_NEW_ROOM,
  name: 'Lab',
  ...change,
});

describe('making rooms', () => {
  it('should offer space members only for a room made in a space', () => {
    expect([joinRulesOffered(false, true), joinRulesOffered(false, false)]).toEqual([
      ['Invite', 'Restricted', 'Knock', 'Public'],
      ['Invite', 'Knock', 'Public'],
    ]);
  });

  it('should offer a space only private or public', () => {
    expect(joinRulesOffered(true, true)).toEqual(['Invite', 'Public']);
  });

  it('should make a space private when asked to make it ask to join', () => {
    expect(createRoomCommand(ADA, draft({ joinRule: 'Knock' }), true)?.data.room.join_rule).toBe(
      'Invite',
    );
  });

  it('should refuse a level beyond what JSON carries exactly', () => {
    expect(parseLevel('-99999999999999999999', 100)).toBeNull();
  });

  it('should not let a hidden address stop a private room', () => {
    expect(
      createRoomCommand(ADA, draft({ joinRule: 'Invite', address: 'not valid' }), false),
    ).not.toBeNull();
  });

  it('should make a private, encrypted room by default', () => {
    expect(createRoomCommand(ADA, draft({ topic: ' Notes ' }), false)).toEqual({
      type: 'CreateRoom',
      data: {
        account: ADA,
        room: {
          name: 'Lab',
          topic: 'Notes',
          space: false,
          parent: null,
          join_rule: 'Invite',
          address: null,
          published: false,
          encrypted: true,
        },
      },
    });
  });

  it('should not make a room without a name', () => {
    expect(createRoomCommand(ADA, draft({ name: '  ' }), false)).toBeNull();
  });

  it('should make a public room unencrypted, with its address and listing', () => {
    const command = createRoomCommand(
      ADA,
      draft({ joinRule: 'Public', address: '#lab', published: true }),
      false,
    );

    expect(command?.data.room).toMatchObject({
      join_rule: 'Public',
      address: 'lab',
      published: true,
      encrypted: false,
    });
  });

  it('should keep a public room encrypted when the user switched it on', () => {
    expect(
      createRoomCommand(ADA, draft({ joinRule: 'Public', encrypted: true }), false)?.data.room
        .encrypted,
    ).toBe(true);
  });

  it('should not give a private room the address typed while it was public', () => {
    expect(
      createRoomCommand(ADA, draft({ address: 'lab', published: true }), false)?.data.room,
    ).toMatchObject({ address: null, published: false });
  });

  it('should let the members of its space into a room made there', () => {
    expect(
      createRoomCommand(ADA, draft({ joinRule: 'Restricted', parent: '!s' }), false)?.data.room,
    ).toMatchObject({ join_rule: 'Restricted', parent: '!s' });
  });

  it('should make a room private when space members has no space', () => {
    expect(
      createRoomCommand(ADA, draft({ joinRule: 'Restricted' }), false)?.data.room.join_rule,
    ).toBe('Invite');
  });

  it('should make a room people ask to join', () => {
    expect(createRoomCommand(ADA, draft({ joinRule: 'Knock' }), false)?.data.room.join_rule).toBe(
      'Knock',
    );
  });

  it('should make a space unencrypted and in no space', () => {
    expect(
      createRoomCommand(ADA, draft({ parent: '!s', joinRule: 'Public' }), true)?.data.room,
    ).toMatchObject({ space: true, parent: null, encrypted: false, join_rule: 'Public' });
  });

  it('should not make a room whose address cannot be one', () => {
    expect(createRoomCommand(ADA, draft({ joinRule: 'Public', address: 'my lab' }), false)).toBe(
      null,
    );
  });

  it('should encrypt by default unless anyone may join', () => {
    expect([
      encryptsByDefault(draft({ joinRule: 'Knock' })),
      encryptsByDefault(draft({ joinRule: 'Public' })),
    ]).toEqual([true, false]);
  });

  it('should read the local part of a typed address', () => {
    expect(addressLocalPart(' #lab:example.org ')).toBe('lab');
  });

  it('should refuse an address with white space', () => {
    expect([addressLocalPart('my lab'), addressInvalid('my lab'), addressInvalid('')]).toEqual([
      null,
      true,
      false,
    ]);
  });

  it('should read the server from a user ID', () => {
    expect(serverOf('@ada:example.org:8448')).toBe('example.org:8448');
  });

  it('should read a join rule from route params, and nothing else', () => {
    expect([joinRuleParam('Public'), joinRuleParam('Open'), joinRuleParam(undefined)]).toEqual([
      'Public',
      null,
      null,
    ]);
  });
});

describe('room settings', () => {
  it('should show every part of a room to someone with every right', () => {
    expect(settingsParts(roomSettings('!a'))).toEqual([
      'general',
      'access',
      'addresses',
      'security',
      'permissions',
      'advanced',
    ]);
  });

  it('should show nothing to someone without rights', () => {
    expect(settingsParts(roomSettings('!a', { rights: rights(false) }))).toEqual([]);
  });

  it('should show only the parts the user has rights to', () => {
    expect(
      settingsParts(roomSettings('!a', { rights: { ...rights(false), topic: true } })),
    ).toEqual(['general']);
  });

  it('should not offer encryption once it is on', () => {
    expect(settingsParts(roomSettings('!a', { encrypted: true }))).not.toContain('security');
  });

  it('should show a space its rooms but no encryption or upgrade', () => {
    expect(settingsParts(roomSettings('!s', { space: true }))).toEqual([
      'general',
      'access',
      'addresses',
      'permissions',
      'rooms',
    ]);
  });

  it('should show access for the directory alone when the server said', () => {
    const only = { ...rights(false), addresses: true };

    expect([
      settingsParts(roomSettings('!a', { rights: only })),
      settingsParts(roomSettings('!a', { rights: only, published: null })),
    ]).toEqual([['access', 'addresses'], ['addresses']]);
  });

  it('should offer space members only when the room is in a space', () => {
    expect([
      joinRulesForSettings(roomSettings('!a', { can_restrict: true })),
      joinRulesForSettings(roomSettings('!a')),
    ]).toEqual([
      ['Invite', 'Restricted', 'Knock', 'Public'],
      ['Invite', 'Knock', 'Public'],
    ]);
  });

  it('should keep space members offered while it is the rule', () => {
    expect(joinRulesForSettings(roomSettings('!a', { can_knock: false }))).not.toContain('Knock');
    expect(joinRulesForSettings(roomSettings('!a', { join_rule: 'Restricted' }))).toContain(
      'Restricted',
    );
  });

  it('should save a changed name and not an unchanged or blank one', () => {
    expect([
      textSetting('Name', 'Lab', ' Studio '),
      textSetting('Name', 'Lab', 'Lab '),
      textSetting('Name', 'Lab', ' '),
    ]).toEqual([{ type: 'Name', data: 'Studio' }, null, null]);
  });

  it('should clear a topic', () => {
    expect(textSetting('Topic', 'Notes', '')).toEqual({ type: 'Topic', data: '' });
  });

  it('should add a typed local part on the account server', () => {
    expect(addressToAdd(roomSettings('!a'), 'lab')).toBe('#lab:example.org');
  });

  it('should add a full address as typed', () => {
    expect(addressToAdd(roomSettings('!a'), 'lab:other.org')).toBe('#lab:other.org');
  });

  it('should not add an address the room has', () => {
    expect(addressToAdd(roomSettings('!a', { main_address: '#lab:example.org' }), '#lab')).toBe(
      null,
    );
  });

  it('should list the main address first', () => {
    expect(
      allAddresses(roomSettings('!a', { main_address: '#b:x', addresses: ['#a:x', '#c:x'] })),
    ).toEqual(['#b:x', '#a:x', '#c:x']);
  });

  it('should show a switch where the user put it while its change is on its way', () => {
    expect(
      withPending(roomSettings('!a'), [{ type: 'GuestAccess', data: true }]).guest_access,
    ).toBe(true);
  });

  it('should make the first address added the main one', () => {
    expect(
      withPending(roomSettings('!a'), [{ type: 'AddAddress', data: '#lab:example.org' }]),
    ).toMatchObject({ main_address: '#lab:example.org', addresses: [] });
  });

  it('should move the old main address among the others when another becomes main', () => {
    expect(
      withPending(roomSettings('!a', { main_address: '#a:x', addresses: ['#b:x'] }), [
        { type: 'MainAddress', data: '#b:x' },
      ]),
    ).toMatchObject({ main_address: '#b:x', addresses: ['#a:x'] });
  });

  it('should show a permission at its new level while it is saved', () => {
    expect(
      withPending(roomSettings('!a'), [
        { type: 'Permission', data: { permission: 'Ban', level: 100 } },
      ]).permissions.find((p) => p.permission === 'Ban')?.level,
    ).toBe(100);
  });
});

describe('permission levels', () => {
  it('should name the roles levels and call any other custom', () => {
    expect([levelChoice(100), levelChoice(50), levelChoice(0), levelChoice(20)]).toEqual([
      'Admin',
      'Moderator',
      'Member',
      'Custom',
    ]);
  });

  it('should clamp a custom level to the user own', () => {
    expect(parseLevel(' 120 ', 50)).toBe(50);
  });

  it('should take negative levels', () => {
    expect(parseLevel('-10', 100)).toBe(-10);
  });

  it('should refuse a level that is not a whole number', () => {
    expect([parseLevel('1.5', 100), parseLevel('ten', 100), parseLevel('', 100)]).toEqual([
      null,
      null,
      null,
    ]);
  });

  it('should offer only the roles up to the user own level', () => {
    expect(choicesOffered(50).map(([choice]) => choice)).toEqual(['Moderator', 'Member']);
  });

  it('should leave a permission the engine marks unchangeable read-only', () => {
    expect(permissionEditable({ permission: 'SendMessages', level: 0, editable: false }, 50)).toBe(
      false,
    );
  });

  it('should leave a permission above the user own level read-only', () => {
    expect(permissionEditable({ permission: 'Upgrade', level: 150, editable: false }, 100)).toBe(
      false,
    );
  });

  it('should set a permission no higher than the user own level', () => {
    expect(permissionSetting(roomSettings('!a', { own_level: 50 }), 'Invite', 100)).toEqual({
      type: 'Permission',
      data: { permission: 'Invite', level: 50 },
    });
  });

  it('should not change a permission above the user own level', () => {
    expect(permissionSetting(roomSettings('!a'), 'Upgrade', 50)).toBeNull();
  });

  it('should not send a permission level that is unchanged', () => {
    expect(permissionSetting(roomSettings('!a'), 'Ban', 50)).toBeNull();
  });

  it('should send sending messages at its shown level, as one of its levels may differ', () => {
    const settings = roomSettings('!a', {
      permissions: [{ permission: 'SendMessages', level: 50, editable: true }],
    });

    expect(permissionSetting(settings, 'SendMessages', 50)).toEqual({
      type: 'Permission',
      data: { permission: 'SendMessages', level: 50 },
    });
  });
});

describe('space rooms', () => {
  const space = spaceSummary('!s', ['!a', '!b', '!c']);

  it('should move a room up one place', () => {
    expect(reorder(space.children, '!b', -1)).toEqual(['!b', '!a', '!c']);
  });

  it('should move a room down one place', () => {
    expect(reorder(space.children, '!b', 1)).toEqual(['!a', '!c', '!b']);
  });

  it('should not move the first room up or the last down', () => {
    expect([reorder(space.children, '!a', -1), reorder(space.children, '!c', 1)]).toEqual([
      null,
      null,
    ]);
  });

  it('should name the children the account knows and mark the others not joined', () => {
    expect(
      spaceChildren(space, [roomSummary('!a', 'alpha')], [spaceSummary('!b', [], [], 'beta')]),
    ).toEqual([
      { roomId: '!a', name: 'alpha', space: false, joined: true },
      { roomId: '!b', name: 'beta', space: true, joined: true },
      { roomId: '!c', name: '!c', space: false, joined: false },
    ]);
  });

  it('should offer the joined group rooms the space does not list', () => {
    const rooms = [
      roomSummary('!a'),
      roomSummary('!d'),
      roomSummary('!dm', 'Grace', 'Direct'),
      { ...roomSummary('!old'), successor: { room_id: '!d', reason: null, via: [] } },
    ];

    expect(roomsToAdd(space, rooms).map((r) => r.room_id)).toEqual(['!d']);
  });

  it('should offer only the joined children to leave along with the space', () => {
    expect(joinedChildren(space, [roomSummary('!c')], []).map((c) => c.roomId)).toEqual(['!c']);
  });

  it('should leave the chosen rooms in the space order', () => {
    expect(leaveSpaceCommand(ADA, space, new Set(['!c', '!a']))).toEqual({
      type: 'LeaveSpace',
      data: { account: ADA, space: '!s', rooms: ['!a', '!c'] },
    });
  });

  it('should leave no rooms along with the space unless chosen', () => {
    expect(leaveSpaceCommand(ADA, space, new Set()).data.rooms).toEqual([]);
  });
});

describe('upgrades', () => {
  const successor = { room_id: '!new', reason: null, via: ['example.org'] };

  it('should open a successor the user already joined', () => {
    expect(successorRoute(successor, [roomSummary('!new')])).toEqual({
      kind: 'open',
      roomId: '!new',
    });
  });

  it('should join a successor through its servers first', () => {
    expect(successorRoute(successor, [])).toEqual({
      kind: 'join',
      roomId: '!new',
      via: ['example.org'],
    });
  });
});
