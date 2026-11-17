import { detachedEngine, type DetachedEngine } from '@better-matrix/matrix-native/client';
import type { AppStateStatus } from 'react-native';

import { Store } from '../../state/store';
import { type AppStates, followFront } from '../front';

/** App states a test moves through by hand. */
function states(first: AppStateStatus): AppStates & { move(to: AppStateStatus): void } {
  const listeners: ((state: AppStateStatus) => void)[] = [];

  return {
    currentState: first,
    addEventListener(_type, listener) {
      listeners.push(listener);
    },
    move(to) {
      for (const listener of listeners) listener(to);
    },
  };
}

function setUp(first: AppStateStatus) {
  const detached = detachedEngine();
  const marker = { show: jest.fn(), hide: jest.fn() };
  const app = states(first);
  followFront(new Store(detached.engine), marker, app);

  return { ...detached, marker, app };
}

function finish({ engine }: DetachedEngine, request: number): Promise<void> {
  engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });

  // The store settles its promise on a later tick.
  return new Promise((resolve) => setTimeout(resolve, 0));
}

describe('followFront', () => {
  it('should tell the engine an app that starts in front is in front', () => {
    const { sent, marker } = setUp('active');
    expect(sent.map((entry) => entry.command)).toEqual([{ type: 'InFront', data: true }]);
    expect(marker.show).toHaveBeenCalledTimes(1);
  });

  it('should keep the marker up until the engine answered that the app left', async () => {
    const detached = setUp('active');
    detached.app.move('background');
    const leaving = detached.sent[1];

    if (!leaving) throw new Error('nothing was sent on leaving');
    expect(leaving.command).toEqual({ type: 'InFront', data: false });
    expect(detached.marker.hide).not.toHaveBeenCalled();
    await finish(detached, leaving.request);
    expect(detached.marker.hide).toHaveBeenCalledTimes(1);
  });

  it('should keep the marker up when the app came back before the engine answered', async () => {
    const detached = setUp('active');
    detached.app.move('background');
    detached.app.move('active');
    const leaving = detached.sent[1];

    if (!leaving) throw new Error('nothing was sent on leaving');
    await finish(detached, leaving.request);
    expect(detached.marker.hide).not.toHaveBeenCalled();
  });

  it('should tell the engine nothing again for a state that changes nothing', () => {
    const { sent, app } = setUp('background');
    app.move('inactive');
    expect(sent.map((entry) => entry.command)).toEqual([{ type: 'InFront', data: false }]);
  });
});
