import { previewGallery } from '../../state/gallery';
import { activeAccount, activeRoomList } from '../../state/selectors';
import { timelineId } from '../../state/timeline';
import { bootDemo } from '../boot';
import { answer, demoEngine, nextAsked } from '../engine';
import {
  ADA,
  ADA_HOBBY,
  DEMO_BATCH,
  DEMO_THREAD,
  DESIGN,
  GENERAL,
  GRACE,
  KAI,
  LAB,
  LAB_V2,
  SCREENSHOT,
  STRANGER_INVITE,
  TEST_PLAN,
  WORK_SPACE,
} from '../fixtures';
import { demoScene } from '../link';
import { scenes, isScene } from '../scenes';

const noMedia = async () => null;

describe('demoScene', () => {
  it('should read the scene from a demo link', () =>
    expect(demoScene('bettermatrix://demo/room')).toBe('room'));
  it('should read the scene from a demo path', () =>
    expect(demoScene('/demo/thread')).toBe('thread'));
  it('should ignore a scene that does not exist', () =>
    expect(demoScene('bettermatrix://demo/nowhere')).toBeNull());
  it('should ignore other links', () =>
    expect(demoScene('org.example.bettermatrix:/oauth?code=1')).toBeNull());
});

describe('scenes', () => {
  it.each(Object.keys(scenes).filter(isScene))('%s should start without an error', (name) => {
    expect(() => bootDemo(name, noMedia)).not.toThrow();
  });

  it('should seed both of Ada’s accounts with Ada’s work on screen', () => {
    const state = bootDemo('home', noMedia).getState();
    expect([state.accounts.map((a) => a.summary.id), state.active]).toEqual([
      [ADA, ADA_HOBBY],
      ADA,
    ]);
  });

  it('should post a batch of four attachments into #design for the gallery scene', () => {
    const design = bootDemo('preview-gallery', noMedia).getState().accounts[0]!.timelines[DESIGN]!;
    expect(previewGallery(design.entries, DEMO_BATCH.second).map((m) => m.event_id)).toEqual(
      Object.values(DEMO_BATCH),
    );
  });

  it('should leave the lone screenshot without a gallery', () => {
    const design = bootDemo('preview', noMedia).getState().accounts[0]!.timelines[DESIGN]!;
    expect(previewGallery(design.entries, SCREENSHOT)).toHaveLength(1);
  });

  it('should fill the general room’s timeline', () => {
    const general = bootDemo('room', noMedia).getState().accounts[0]!.timelines[GENERAL]!;
    expect(general.entries.some((e) => e.kind.type === 'Message')).toBe(true);
  });

  it('should fill the demo thread', () => {
    const id = timelineId({ room_id: DEMO_THREAD.roomId, thread: DEMO_THREAD.root });
    expect(
      bootDemo('thread', noMedia).getState().accounts[0]!.timelines[id]!.entries.length,
    ).toBeGreaterThan(1);
  });

  it('should put the unverified account on screen for its scene', () => {
    expect(
      activeAccount(bootDemo('unverified', noMedia).getState())?.security?.session_verified,
    ).toBe(false);
  });

  it('should start signed out for the sign-in scene', () => {
    const state = bootDemo('sign-in', noMedia).getState();
    expect([state.started, state.accounts.length]).toEqual([true, 0]);
  });

  it('should follow the speaker in the focus scene', () => {
    expect(bootDemo('call-focus', noMedia).getState().call.view.layout).toEqual({
      type: 'Focus',
      pin: null,
    });
  });

  it('should show the chat under the call in the call chat scene', () => {
    const state = bootDemo('call-chat', noMedia).getState();
    expect([state.call.view.chat, state.call.current?.room_id]).toEqual([true, GENERAL]);
  });

  it('should crowd the many scene enough to page', () => {
    expect(bootDemo('call-many', noMedia).getState().call.current?.participants).toHaveLength(29);
  });

  it('should apply a scene’s language', () => {
    expect(bootDemo('de', noMedia).getState().settings.language).toBe('de');
  });
});

describe('the demo engine', () => {
  it('should answer a discovery with a server offering both sign-ins', () => {
    const [event] = answer({ type: 'Discover', data: { server: 'example.org' } }, 4);
    expect(event).toMatchObject({
      type: 'HomeserverDiscovered',
      data: { request: 4, homeserver: { methods: { oauth: true, password: true } } },
    });
  });

  it('should show a sent message in its timeline', () => {
    const key = { room_id: GENERAL, thread: null };

    const [event] = answer(
      {
        type: 'Send',
        data: {
          account: ADA,
          key,
          reply_to: null,
          message: { type: 'Text', data: { body: 'hi', html: null } },
        },
      },
      9,
    );

    expect(event).toMatchObject({
      type: 'TimelineChanged',
      data: { key, diffs: [{ type: 'PushBack' }] },
    });
  });

  it('should find people by name', () => {
    const [event] = answer({ type: 'SearchPeople', data: { account: ADA, term: 'grace' } }, 2);
    expect(event?.type === 'PeopleFound' ? event.data.people.map((p) => p.name) : []).toEqual([
      'Grace Hopper',
    ]);
  });

  it('should preview a linked room nobody in the demo joined', () => {
    const [event] = answer(
      {
        type: 'PreviewRoom',
        data: { account: ADA, room: '#rust:matrix.org', via: ['matrix.org'] },
      },
      3,
    );

    expect(event).toMatchObject({
      type: 'RoomPreview',
      data: { request: 3, preview: { name: 'Rust', joined: false } },
    });
  });

  it('should preview the general room as joined by its alias', () => {
    const [event] = answer(
      { type: 'PreviewRoom', data: { account: ADA, room: '#general:example.org', via: [] } },
      3,
    );

    expect(event).toMatchObject({
      type: 'RoomPreview',
      data: { preview: { room_id: GENERAL, joined: true } },
    });
  });

  it('should join a linked room by the ID its preview names', () => {
    const [event] = answer(
      { type: 'JoinRoom', data: { account: ADA, room_id: '#rust:matrix.org', via: [] } },
      5,
    );

    expect(event).toEqual({
      type: 'RoomJoined',
      data: { request: 5, room_id: '!rust-lang:matrix.org' },
    });
  });

  it('should name the flow a verification request started before its steps', () => {
    const events = answer({ type: 'VerifyThisSession', data: ADA }, 6);
    expect(events.map((e) => e.type)).toEqual(['VerificationRequested', 'Verification']);
    expect(events[0]).toEqual({
      type: 'VerificationRequested',
      data: { request: 6, account: ADA, flow_id: 'demo-self' },
    });
  });

  it('should start the verification of a person under the flow it named', () => {
    const events = answer({ type: 'VerifyUser', data: { account: ADA, user_id: KAI } }, 7);

    const flows = events.map((e) =>
      e.type === 'VerificationRequested' || e.type === 'Verification' ? e.data.flow_id : null,
    );

    expect(flows).toEqual([`demo-${KAI}`, `demo-${KAI}`]);
  });

  it('should ask for an email address after the registration token', () => {
    expect(nextAsked({ type: 'RegistrationToken', data: 't' }, 3)).toEqual([
      { type: 'Registration', data: { request: 3, step: { type: 'Email' } } },
    ]);
  });

  it('should say where it sent the link for the email address given', () => {
    expect(nextAsked({ type: 'Email', data: 'ada@example.org' }, 3)).toEqual([
      {
        type: 'Registration',
        data: {
          request: 3,
          step: { type: 'EmailSent', data: { address: 'ada@example.org', unconfirmed: false } },
        },
      },
    ]);
  });

  it('should finish every command', async () => {
    const engine = demoEngine(noMedia);
    const finished: number[] = [];
    engine.subscribe((e) => {
      if (e.type === 'Finished') finished.push(e.data.request);
    });

    const request = engine.send({
      type: 'MarkRead',
      data: {
        account: ADA,
        key: { room_id: GENERAL, thread: null },
        event_id: null,
        fully_read: false,
      },
    });

    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(finished).toEqual([request]);
  });
});

it('should list a newly pinned message with its real preview', async () => {
  const store = bootDemo('room', noMedia);
  await store.run({
    type: 'SetPinned',
    data: { account: ADA, room_id: GENERAL, event_id: TEST_PLAN, pinned: true },
  });
  await store.run({ type: 'ListPinnedMessages', data: { account: ADA, room_id: GENERAL } });
  expect(store.getState().accounts[0]?.pins[GENERAL]).toEqual(
    expect.arrayContaining([
      expect.objectContaining({
        event_id: TEST_PLAN,
        body: expect.stringContaining('Encryption test plan'),
      }),
    ]),
  );
});

it('should answer a safety change with the choices after it', async () => {
  const store = bootDemo('settings-safety', noMedia);
  await store.run({
    type: 'SetSafety',
    data: { account: ADA, change: { type: 'Ignore', data: { user_id: GRACE, ignored: true } } },
  });
  expect(store.getState().accounts[0]?.safety.ignored).toContain(GRACE);
  expect(store.getState().accounts[0]?.safety.media).toBe('Private');
});

it('should hide the stranger’s invitation in the safety scenes', () => {
  const state = bootDemo('settings-safety', noMedia).getState();
  expect(activeRoomList(state)?.invitations.map((i) => i.room_id)).not.toContain(STRANGER_INVITE);
});

describe('the demo engine as a homeserver for administration', () => {
  it('should load a room settings and change one', async () => {
    const store = bootDemo('room-info', noMedia);
    await store.run({ type: 'LoadRoomSettings', data: { account: ADA, room_id: GENERAL } });
    await store.run({
      type: 'ChangeRoom',
      data: { account: ADA, room_id: GENERAL, setting: { type: 'Name', data: 'lobby' } },
    });
    const account = store.getState().accounts[0];
    expect([
      account?.roomSettings[GENERAL]?.name,
      account?.rooms.find((r) => r.room_id === GENERAL)?.name,
    ]).toEqual(['lobby', 'lobby']);
  });

  it('should order a space rooms', async () => {
    const store = bootDemo('home', noMedia);
    const order = [DESIGN, GENERAL, '!releases:example.org'];
    await store.run({
      type: 'ChangeSpace',
      data: { account: ADA, space: WORK_SPACE, change: { type: 'Order', data: order } },
    });
    expect(
      store.getState().accounts[0]?.spaces.find((s) => s.room_id === WORK_SPACE)?.children,
    ).toEqual(order);
  });

  it('should leave a space with the rooms chosen', async () => {
    const store = bootDemo('home', noMedia);
    await store.run({
      type: 'LeaveSpace',
      data: { account: ADA, space: WORK_SPACE, rooms: [DESIGN] },
    });
    const account = store.getState().accounts[0];
    expect([
      account?.spaces.some((s) => s.room_id === WORK_SPACE),
      account?.rooms.some((r) => r.room_id === DESIGN),
      account?.rooms.some((r) => r.room_id === GENERAL),
    ]).toEqual([false, false, true]);
  });

  it('should approve a request to join #design', async () => {
    const store = bootDemo('member-requests', noMedia);
    await store.run({
      type: 'ModerateMember',
      data: {
        account: ADA,
        room_id: DESIGN,
        user_id: '@lin:example.org',
        action: { type: 'Admit' },
      },
    });
    await store.run({ type: 'ListMembers', data: { account: ADA, room_id: DESIGN, all: true } });
    const members = store.getState().accounts[0]?.members[DESIGN];
    expect([
      members?.knocking.map((m) => m.person.id),
      members?.people.some((m) => m.person.id === '@lin:example.org'),
    ]).toEqual([['@sam:hobby.chat'], true]);
  });

  it('should point an upgraded room to its successor', async () => {
    const store = bootDemo('room-settings-upgrade', noMedia);
    await store.run({ type: 'UpgradeRoom', data: { account: ADA, room_id: LAB, version: '12' } });
    const lab = store.getState().accounts[0]?.rooms.find((r) => r.room_id === LAB);
    expect(lab?.successor?.room_id).toBe('!lab-v12:example.org');
  });

  it('should close a replaced room to upgrades and new levels', async () => {
    const store = bootDemo('home', noMedia);
    await store.run({ type: 'UpgradeRoom', data: { account: ADA, room_id: LAB, version: '12' } });
    await store.run({ type: 'LoadRoomSettings', data: { account: ADA, room_id: LAB } });
    const settings = store.getState().accounts[0]?.roomSettings[LAB];
    expect([settings?.upgrade_to, settings?.permissions.some((p) => p.editable)]).toEqual([
      null,
      false,
    ]);
    await expect(
      store.run({
        type: 'ChangeRoom',
        data: {
          account: ADA,
          room_id: LAB,
          setting: { type: 'Permission', data: { permission: 'SendMessages', level: 0 } },
        },
      }),
    ).rejects.toThrow();
  });

  it('should make a room created public a public one', async () => {
    const store = bootDemo('home', noMedia);
    await store.run({
      type: 'CreateRoom',
      data: {
        account: ADA,
        room: {
          name: 'Open',
          topic: null,
          space: false,
          parent: null,
          join_rule: 'Public',
          address: null,
          published: false,
          encrypted: false,
        },
      },
    });
    const open = store.getState().accounts[0]?.rooms.find((r) => r.name === 'Open');
    expect(open?.public).toBe(true);
    await store.run({
      type: 'LoadRoomSettings',
      data: { account: ADA, room_id: open?.room_id ?? '' },
    });
    const settings = store.getState().accounts[0]?.roomSettings[open?.room_id ?? ''];
    expect([settings?.join_rule, settings?.encrypted]).toEqual(['Public', false]);
  });

  it('should make a space created public a public one in its settings', async () => {
    const store = bootDemo('home', noMedia);
    await store.run({
      type: 'CreateRoom',
      data: {
        account: ADA,
        room: {
          name: 'Commons',
          topic: null,
          space: true,
          parent: null,
          join_rule: 'Public',
          address: null,
          published: false,
          encrypted: false,
        },
      },
    });
    const made = store.getState().accounts[0]?.spaces.find((s) => s.name === 'Commons');
    await store.run({
      type: 'LoadRoomSettings',
      data: { account: ADA, room_id: made?.room_id ?? '' },
    });
    expect(store.getState().accounts[0]?.roomSettings[made?.room_id ?? '']?.join_rule).toBe(
      'Public',
    );
  });

  it('should keep a replaced room closed after another change to it', async () => {
    const store = bootDemo('home', noMedia);
    await store.run({ type: 'UpgradeRoom', data: { account: ADA, room_id: LAB, version: '12' } });
    await store.run({
      type: 'ChangeRoom',
      data: { account: ADA, room_id: LAB, setting: { type: 'Topic', data: 'Moved' } },
    });
    expect(store.getState().accounts[0]?.roomSettings[LAB]?.upgrade_to).toBeNull();
  });

  it('should show the tombstone scene room as replaced', () => {
    const lab = bootDemo('tombstone', noMedia)
      .getState()
      .accounts[0]?.rooms.find((r) => r.room_id === LAB);

    expect(lab?.successor?.room_id).toBe(LAB_V2);
  });
});
