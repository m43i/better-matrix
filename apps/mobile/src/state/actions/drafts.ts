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
      if (event.type === 'AccountRemoved') this.forget(event.data);

      if (event.type !== 'DraftLoaded' || event.data.draft === null) return;
      const id = composerId(event.data.account, event.data.key);

      if (!this.owners.has(id)) return;
      const composer = store.getState().composers[id];

      if (composer?.touched) {
        // The user's changes kept it out: what they left (even nothing) is written over it.
        this.kept.set(id, event.data.draft);
        this.write(id);
      } else if (sameDraft(storedDraft(composer), event.data.draft)) {
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

  /** Writes the draft of `key` now, if it changed: its conversation is being left. */
  flush(account: AccountId, key: TimelineKey): void {
    const id = composerId(account, key);

    if (this.timers.has(id)) this.write(id);
  }

  /** An account went: its composers are no longer followed, and nothing of it is written. */
  private forget(account: AccountId): void {
    for (const id of [...this.owners.keys()].filter((id) => id.startsWith(`${account}|`))) {
      this.cancel(id);
      this.owners.delete(id);
      this.kept.delete(id);
      this.seen.delete(id);
    }
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

function keeperOf(store: Store): DraftKeeper {
  let keeper = keepers.get(store);

  if (!keeper) {
    keeper = new DraftKeeper(store);
    keepers.set(store, keeper);
  }

  return keeper;
}

/** Keeps the draft of the composer of `key` in the store, from now on in this run. */
export function followDraft(store: Store, account: AccountId, key: TimelineKey): void {
  keeperOf(store).follow(account, key);
}

/** Writes the draft of `key` without waiting: its conversation is being left. */
export function flushDraft(store: Store, account: AccountId, key: TimelineKey): void {
  keeperOf(store).flush(account, key);
}
