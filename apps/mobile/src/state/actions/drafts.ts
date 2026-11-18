// Keeping each composer's draft in the engine's store, so a draft survives the app being closed
// (the desktop's `state/drafts.rs`): a composer asks for its kept draft the first time it shows
// in a run, and its draft is written back as it changes, at most every `KEEP_EVERY_MS`, an
// emptied one (sent or cleared) at once, so a sent message never comes back as a draft.

import type { AccountId, Draft, TimelineKey } from '@better-matrix/matrix-native/types';

import { type ComposerState, composerId } from '../composer';
import { KEEP_EVERY_MS, sameDraft, storedDraft } from '../drafts';
import type { Store } from '../store';

interface Owner {
  account: AccountId;
  key: TimelineKey;
}

class DraftKeeper {
  /** The composers shown this run, by `composerId`. */
  private readonly owners = new Map<string, Owner>();
  /** The draft the store holds per composer, as far as the app knows; absent for none. */
  private readonly kept = new Map<string, Draft>();
  /** Each composer as last looked at, so only changed ones are compared. */
  private readonly seen = new Map<string, ComposerState | undefined>();
  private readonly timers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(private readonly store: Store) {
    store.subscribe(() => this.check());
    store.addEffect((event) => {
      if (event.type !== 'DraftLoaded' || event.data.draft === null) return;
      const id = composerId(event.data.account, event.data.key);

      // Only a draft that went into its composer is known to be what the store holds: one the
      // user's typing kept out is written over in turn.
      if (sameDraft(storedDraft(store.getState().composers[id]), event.data.draft)) {
        this.kept.set(id, event.data.draft);
      }
    });
  }

  /** A composer shows: the first time this run, its kept draft is asked for. */
  follow(account: AccountId, key: TimelineKey): void {
    const id = composerId(account, key);

    if (this.owners.has(id)) return;
    this.owners.set(id, { account, key });
    this.seen.set(id, this.store.getState().composers[id]);
    void this.store.run({ type: 'LoadDraft', data: { account, key } }).catch(() => {
      // A failed read is asked again the next time the composer shows.
      this.owners.delete(id);
    });
  }

  private check(): void {
    const composers = this.store.getState().composers;

    for (const id of this.owners.keys()) {
      const composer = composers[id];

      if (this.seen.get(id) === composer) continue;
      this.seen.set(id, composer);

      const draft = storedDraft(composer);

      if (sameDraft(draft, this.kept.get(id) ?? null)) {
        this.cancel(id);
      } else if (draft === null) {
        this.write(id);
      } else if (!this.timers.has(id)) {
        this.timers.set(
          id,
          setTimeout(() => this.write(id), KEEP_EVERY_MS),
        );
      }
    }
  }

  private cancel(id: string): void {
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
  }

  private write(id: string): void {
    this.cancel(id);
    const owner = this.owners.get(id);

    if (!owner) return;
    const draft = storedDraft(this.store.getState().composers[id]);

    if (sameDraft(draft, this.kept.get(id) ?? null)) return;

    if (draft) this.kept.set(id, draft);
    else this.kept.delete(id);
    this.store.send(
      { type: 'SaveDraft', data: { account: owner.account, key: owner.key, draft } },
      { quiet: true },
    );
  }
}

const keepers = new WeakMap<Store, DraftKeeper>();

/** Keeps the draft of the composer of `key` in the store, from now on in this run. */
export function followDraft(store: Store, account: AccountId, key: TimelineKey): void {
  let keeper = keepers.get(store);

  if (!keeper) {
    keeper = new DraftKeeper(store);
    keepers.set(store, keeper);
  }

  keeper.follow(account, key);
}
