// The engine owns the room beacons and ends each when its duration is up. One native device
// watch serves their active requests until the last one finishes.
import type { LiveDuration } from '@better-matrix/matrix-native/types';

import { addNotice } from '../state/reducer';
import type { Store } from '../state/store';

export const shareKey = (account: string, roomId: string) => JSON.stringify([account, roomId]);

interface Share {
  store: Store;
  key: string;
  request?: number;
  pendingFix?: string;
  state: 'starting' | 'receiving' | 'stopping';
}

export class LiveLocations {
  private readonly shares = new Set<Share>();
  private changing?: Promise<void>;

  constructor(private readonly setWatching: (wanted: boolean) => Promise<void>) {}

  private updateWatch() {
    const apply = () =>
      this.setWatching([...this.shares].some((share) => share.state !== 'stopping'));

    // Begin the first native call from the button handler. Serialize subsequent transitions.
    const update = this.changing ? this.changing.then(apply, apply) : apply();
    this.changing = update;

    const clear = () => {
      if (this.changing === update) this.changing = undefined;
    };

    void update.then(clear, clear);

    return update;
  }

  async deviceFix(uri: string | null) {
    // A headless restart with no owning engine must unregister an orphaned watch.
    if (!this.shares.size) return this.updateWatch();

    for (const share of this.shares) {
      if (share.state === 'stopping') continue;

      if (uri === null) this.stopShare(share);
      else if (share.state === 'starting') share.pendingFix = uri;
      else this.publish(share, uri);
    }
  }

  private publish({ store, request }: Share, uri: string) {
    if (request === undefined) return;
    store.send(
      { type: 'Answer', data: { request, answer: { type: 'LocationFix', data: uri } } },
      { quiet: true },
    );
  }

  private stopShare(share: Share) {
    if (share.state === 'stopping') return;
    share.state = 'stopping';
    void this.updateWatch().catch(() => {});

    if (share.request !== undefined)
      share.store.send({ type: 'Cancel', data: share.request }, { quiet: true });
  }

  stop(store: Store, request: number) {
    for (const share of this.shares) {
      if (share.store === store && share.request === request) this.stopShare(share);
    }
  }

  async start(
    store: Store,
    account: string,
    roomId: string,
    first: string,
    duration: LiveDuration,
  ) {
    const key = shareKey(account, roomId);

    if ([...this.shares].some((share) => share.store === store && share.key === key)) return false;
    const share: Share = { store, key, state: 'starting' };
    this.shares.add(share);

    const unsubscribe = store.addEffect((event) => {
      if (
        (event.type === 'AccountRemoved' && event.data === account) ||
        (event.type === 'SessionEnded' && event.data.account === account)
      )
        this.stopShare(share);
    });

    const release = () => {
      this.shares.delete(share);
      unsubscribe();
      void this.updateWatch().catch(() => {});

      if (share.request === undefined) return;
      store.update((s) => {
        const liveShares = { ...s.liveShares };
        delete liveShares[key];

        return { ...s, liveShares };
      });
    };

    try {
      await this.updateWatch();

      if (share.state === 'stopping') {
        release();

        return false;
      }

      const run = store.run(
        {
          type: 'ShareLiveLocation',
          data: { account, room_id: roomId, geo_uri: first, duration },
        },
        (event) => {
          // A share until stopped starts a fresh beacon before each one runs out: the newest
          // is the one to stop.
          if (event.type !== 'LiveLocationStarted' || share.state === 'stopping') return;
          share.state = 'receiving';
          store.update((s) => ({
            ...s,
            liveShares: {
              ...s.liveShares,
              [key]: { request: event.data.request, eventId: event.data.event_id },
            },
          }));

          if (share.pendingFix) this.publish(share, share.pendingFix);
          delete share.pendingFix;
        },
      );

      share.request = run.request;
      store.update((s) => ({
        ...s,
        liveShares: { ...s.liveShares, [key]: { request: run.request, eventId: null } },
      }));
      void run
        .catch((error: Error) => store.update((s) => addNotice(s, error.message, account)))
        .finally(release);

      return true;
    } catch (error) {
      release();
      store.update((s) => addNotice(s, String(error), account));

      return false;
    }
  }
}
