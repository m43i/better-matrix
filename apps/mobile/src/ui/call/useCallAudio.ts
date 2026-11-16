// Who is speaking and who has speaker focus, followed from the store's levels for whichever
// stage shows (full or compact). One follower per store, so going between the stage and the chat
// keeps the speaker; it resets when another call starts. The levels come every 100 ms, but the
// stage redraws only when the speaking rings or speaker focus change (`hearCall`).

import { useSyncExternalStore } from 'react';

import { type CallAudio, focusedSpeaker, hearCall, initialCallAudio } from '../../state/callView';
import { useAppStore } from '../../state/react';
import type { Store } from '../../state/store';

export interface HeardCall {
  /** Who is speaking now, by participant key ("" for the user). */
  speaking: ReadonlySet<string>;
  /** The remote participant with speaker focus, for Focus without a pin. */
  speaker: string | null;
}

const QUIET: HeardCall = { speaking: new Set(), speaker: null };

class Follower {
  private audio: CallAudio = initialCallAudio();
  private call: number | null = null;
  private heard: HeardCall = QUIET;
  private listeners = new Set<() => void>();
  private stop: (() => void) | null = null;

  constructor(private readonly store: Store) {}

  snapshot = (): HeardCall => this.heard;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    if (!this.stop) this.start();
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) {
        this.stop?.();
        this.stop = null;
      }
    };
  };

  private start(): void {
    let levels = this.store.getState().call.levels;
    let current = this.store.getState().call.current;
    this.tick();
    const unsubscribe = this.store.subscribe(() => {
      const state = this.store.getState().call;
      if (state.levels === levels && state.current === current) return;
      levels = state.levels;
      current = state.current;
      this.tick();
    });
    // Someone going quiet stops speaking after the hold, even if no new levels come.
    const timer = setInterval(() => this.tick(), 200);
    this.stop = () => {
      unsubscribe();
      clearInterval(timer);
    };
  }

  private tick(): void {
    const { current, levels } = this.store.getState().call;
    const call = current?.call ?? null;
    if (call !== this.call) {
      this.call = call;
      this.audio = initialCallAudio();
      this.publish();
    }
    if (!current) return;
    const heard = hearCall(this.audio, levels, current.participants, Date.now());
    this.audio = heard.audio;
    if (heard.changed) this.publish();
  }

  private publish(): void {
    this.heard = { speaking: new Set(this.audio.speaking), speaker: focusedSpeaker(this.audio.focus) };
    for (const listener of this.listeners) listener();
  }
}

const followers = new WeakMap<Store, Follower>();

/** Who speaks, and who has speaker focus, in the call under way. */
export function useCallAudio(): HeardCall {
  const store = useAppStore();
  let follower = followers.get(store);
  if (!follower) {
    follower = new Follower(store);
    followers.set(store, follower);
  }
  return useSyncExternalStore(follower.subscribe, follower.snapshot);
}
