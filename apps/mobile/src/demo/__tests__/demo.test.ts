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
  SCREENSHOT,
  STRANGER_INVITE,
  TEST_PLAN,
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
