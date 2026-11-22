import { previewGallery } from '../../state/gallery';
import { activeAccount } from '../../state/selectors';
import { timelineId } from '../../state/timeline';
import { bootDemo } from '../boot';
import { answer, demoEngine } from '../engine';
import {
  ADA,
  ADA_HOBBY,
  DEMO_BATCH,
  DEMO_THREAD,
  DESIGN,
  GENERAL,
  RELEASES,
  SCREENSHOT,
  SIGN_OFF,
  TEST_PLAN,
  THREAD_REPLY,
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
          message: {
            type: 'Text',
            data: { body: 'hi', html: null, mentions: { user_ids: [], room: false } },
          },
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

describe('the demo engine’s composer extras', () => {
  it('should list Ada’s emoji and the room’s stickers', () => {
    const [event] = answer({ type: 'ListImagePacks', data: { account: ADA, room_id: GENERAL } }, 1);
    expect(event).toMatchObject({ type: 'ImagePacks', data: { packs: [{ id: 'user' }, {}] } });
  });

  it('should find places by name', () => {
    const [event] = answer({ type: 'SearchPlaces', data: { query: 'tor', language: null } }, 2);
    expect(event).toMatchObject({
      type: 'PlacesFound',
      data: { request: 2, places: [{ geo_uri: 'geo:52.516275,13.377704' }] },
    });
  });

  it('should list the versions of Ada’s sign-off', () => {
    const [event] = answer(
      { type: 'ListEdits', data: { account: ADA, room_id: RELEASES, event_id: SIGN_OFF } },
      3,
    );

    expect(event?.type === 'EditHistory' && event.data.versions.length).toBe(3);
  });

  it('should find a reply in its thread', () => {
    const [event] = answer(
      { type: 'LocateMessage', data: { account: ADA, room_id: GENERAL, event_id: THREAD_REPLY } },
      4,
    );

    expect(event).toMatchObject({
      type: 'MessageLocated',
      data: { key: { room_id: GENERAL, thread: DEMO_THREAD.root } },
    });
  });

  it('should hold a scheduled message and send it on request', async () => {
    const engine = demoEngine(noMedia);
    const heard: string[] = [];
    engine.subscribe((event) => {
      if (event.type === 'ScheduledMessages') heard.push(String(event.data.messages.length));

      if (event.type === 'TimelineChanged') heard.push('sent');
    });

    const message = {
      type: 'Text' as const,
      data: { body: 'later', html: null, mentions: { user_ids: [], room: false } },
    };

    engine.send({
      type: 'Schedule',
      data: {
        account: ADA,
        key: { room_id: GENERAL, thread: null },
        reply_to: null,
        message,
        delay_ms: 60_000,
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    engine.send({
      type: 'UpdateScheduled',
      data: { account: ADA, delay_id: 'demo-delay-1', action: 'SendNow' },
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(heard).toEqual(['1', 'sent', '0']);
  });
});
