// Keeping each composer's draft in the engine's store, so a draft survives the app being closed
// (the desktop's `state/drafts.rs`): a composer asks for its kept draft the first time it shows
// in a run, and its draft is written back as it changes, at most every `KEEP_EVERY_MS`, an
// emptied one (sent or cleared) at once, so a sent message never comes back as a draft.

import type { AccountId, Draft, TimelineKey } from '@better-matrix/matrix-native/types';

import { type ComposerState, composerId } from '../composer';
import { KEEP_EVERY_MS, sameDraft, storedDraft, withKeptDraft } from '../drafts';
import type { Store } from '../store';

interface Owner {
  account: AccountId;
  key: TimelineKey;
  /** This follow's own: a read answered after the account left (and came back) is ignored. */
  token: object;
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
    });
  }

  /** A composer shows: the first time this run, its kept draft is asked for. */
  follow(account: AccountId, key: TimelineKey): void {
    const id = composerId(account, key);

    if (this.owners.has(id)) return;
    const token = {};
    this.owners.set(id, { account, key, token });
    this.seen.set(id, this.store.getState().composers[id]);
    const ours = () => this.owners.get(id)?.token === token;
    void this.store
      .run({ type: 'LoadDraft', data: { account, key } }, (event) => {
        if (event.type === 'DraftLoaded' && event.data.draft && ours())
          this.loaded(id, event.data.draft);
      })
      .catch(() => {
        // A failed read is asked again the next time the composer shows.
        if (ours()) this.owners.delete(id);
      });
  }

  /**
   * The store's draft for `id` came: it goes back into the composer unless the user changed the
   * draft meanwhile; then what they left (even nothing) is written over it.
   */
  private loaded(id: string, draft: Draft): void {
    const composer = this.store.getState().composers[id];

    if (composer?.touched) {
      this.kept.set(id, draft);
      this.write(id);

      return;
    }

    const next = withKeptDraft(composer, draft);

    if (next !== composer) {
      this.store.update((s) => ({ ...s, composers: { ...s.composers, [id]: next } }));
      this.kept.set(id, draft);
    }
  }

  /** Writes the draft of `key` now, if it changed: its conversation is being left. */
  flush(account: AccountId, key: TimelineKey): void {
    this.write(composerId(account, key));
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
