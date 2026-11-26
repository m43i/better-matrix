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

  const start = (room: string) => {
    live.start(store, '@me:test', room, 'geo:1,2');
    const request = sent.at(-1)!.request;
    engine.emit({ type: 'LiveLocationStarted', data: { request } });

    return request;
  };

  const first = start('!one:test');
  const second = start('!two:test');
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
