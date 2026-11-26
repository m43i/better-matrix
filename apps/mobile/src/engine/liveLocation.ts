// The engine owns the room beacons. One native device watch serves their active requests.
import { addNotice } from '../state/reducer';
import type { Store } from '../state/store';

export const shareKey = (account: string, roomId: string) => JSON.stringify([account, roomId]);

interface Share {
  store: Store;
  request: number;
  state: 'starting' | 'receiving' | 'stopping';
}

export class LiveLocations {
  private readonly shares = new Set<Share>();
  private changing = Promise.resolve();

  constructor(private readonly setWatching: (wanted: boolean) => Promise<void>) {}

  private updateWatch() {
    const update = this.changing.then(() =>
      this.setWatching([...this.shares].some((share) => share.state === 'receiving')),
    );

    this.changing = update.catch(() => {});

    return update;
  }

  async deviceFix(uri: string | null) {
    // A headless restart with no owning engine must unregister an orphaned watch.
    if (!this.shares.size) return this.updateWatch();

    for (const { store, request, state } of this.shares) {
      if (state !== 'receiving') continue;

      if (uri === null) this.stop(store, request);
      else
        store.send(
          { type: 'Answer', data: { request, answer: { type: 'LocationFix', data: uri } } },
          { quiet: true },
        );
    }
  }

  stop(store: Store, request: number) {
    for (const share of this.shares) {
      if (share.store === store && share.request === request) share.state = 'stopping';
    }

    void this.updateWatch().catch(() => {});
    store.send({ type: 'Cancel', data: request }, { quiet: true });
  }

  start(store: Store, account: string, roomId: string, first: string) {
    const key = shareKey(account, roomId);

    if (store.getState().liveShares[key] != null) return;

    const run = store.run(
      { type: 'ShareLiveLocation', data: { account, room_id: roomId, geo_uri: first } },
      (event) => {
        if (event.type !== 'LiveLocationStarted' || share.state !== 'starting') return;
        share.state = 'receiving';
        void this.updateWatch().catch((error: Error) => {
          store.update((s) => addNotice(s, error.message, account));
          this.stop(store, run.request);
        });
      },
    );

    const share: Share = { store, request: run.request, state: 'starting' };
    this.shares.add(share);

    const effects = store.addEffect((event) => {
      if (
        (event.type === 'AccountRemoved' && event.data === account) ||
        (event.type === 'SessionEnded' && event.data.account === account)
      )
        this.stop(store, run.request);
    });

    store.update((s) => ({ ...s, liveShares: { ...s.liveShares, [key]: run.request } }));
    void run
      .catch((error: Error) => store.update((s) => addNotice(s, error.message, account)))
      .finally(() => {
        this.shares.delete(share);
        void this.updateWatch().catch(() => {});
        effects();
        store.update((s) => {
          const liveShares = { ...s.liveShares };
          delete liveShares[key];

          return { ...s, liveShares };
        });
      });
  }
}
