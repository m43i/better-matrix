import { detachedEngine } from '@better-matrix/matrix-native/client';
import { Store } from '../../state/store';
import { LiveLocations } from '../liveLocation';

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

it('should send the latest device fix only to rooms still sharing and release the last watch', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  let running = false;

  const live = new LiveLocations(async (wanted) => {
    running = wanted;
  });

  const start = async (room: string) => {
    await live.start(store, '@me:test', room, 'geo:1,2', 'Hour');
    const request = sent.at(-1)!.request;
    engine.emit({ type: 'LiveLocationStarted', data: { request, event_id: room } });

    return request;
  };

  const first = await start('!one:test');
  const second = await start('!two:test');
  await tick();
  expect(running).toBe(true);
  live.stop(store, first);
  await live.deviceFix('geo:5,6;u=8');
  expect(
    sent.filter(({ command }) => command.type === 'Answer').map(({ command }) => command),
  ).toEqual([
    {
      type: 'Answer',
      data: { request: second, answer: { type: 'LocationFix', data: 'geo:5,6;u=8' } },
    },
  ]);

  for (const request of [first, second])
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
  await tick();
  expect(running).toBe(false);
  expect(store.getState().liveShares).toEqual({});
});

it('should start tracking before the homeserver responds and publish the most recent waiting fix', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const changes: boolean[] = [];

  const live = new LiveLocations(async (wanted) => {
    changes.push(wanted);
  });

  await live.start(store, '@me:test', '!room:test', 'geo:1,2', 'Hour');
  const request = sent[0]!.request;
  expect(changes).toEqual([true]);
  await live.deviceFix('geo:2,3');
  await live.deviceFix('geo:3,4');
  expect(sent).toHaveLength(1);
  engine.emit({ type: 'LiveLocationStarted', data: { request, event_id: '$session' } });
  expect(sent.at(-1)?.command).toEqual({
    type: 'Answer',
    data: { request, answer: { type: 'LocationFix', data: 'geo:3,4' } },
  });
  expect(Object.values(store.getState().liveShares)).toEqual([{ request, eventId: '$session' }]);
});

it('should release native tracking if the homeserver rejects the beacon', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const changes: boolean[] = [];

  const live = new LiveLocations(async (wanted) => {
    changes.push(wanted);
  });

  await live.start(store, '@me:test', '!room:test', 'geo:1,2', 'Hour');
  engine.emit({
    type: 'Finished',
    data: { request: sent[0]!.request, result: { Err: 'Forbidden' } },
  });
  await tick();
  expect(changes).toEqual([true, false]);
  expect(store.getState().liveShares).toEqual({});
});

it('should leave the room untouched when native tracking cannot start', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);

  const live = new LiveLocations(async (wanted) => {
    if (wanted) throw new Error('Permission denied');
  });

  expect(await live.start(store, '@me:test', '!room:test', 'geo:1,2', 'Hour')).toBe(false);
  expect(sent).toHaveLength(0);
  expect(store.getState().liveShares).toEqual({});
});

it('should ask the engine to share for the chosen duration', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const live = new LiveLocations(async () => {});

  await live.start(store, '@me:test', '!room:test', 'geo:1,2', 'UntilStopped');
  expect(sent[0]?.command).toEqual({
    type: 'ShareLiveLocation',
    data: {
      account: '@me:test',
      room_id: '!room:test',
      geo_uri: 'geo:1,2',
      duration: 'UntilStopped',
    },
  });
});

it('should make a renewed beacon the one to stop', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const live = new LiveLocations(async () => {});

  await live.start(store, '@me:test', '!room:test', 'geo:1,2', 'UntilStopped');
  const request = sent[0]!.request;
  engine.emit({ type: 'LiveLocationStarted', data: { request, event_id: '$first' } });
  engine.emit({ type: 'LiveLocationStarted', data: { request, event_id: '$renewed' } });
  expect(Object.values(store.getState().liveShares)).toEqual([{ request, eventId: '$renewed' }]);
});
