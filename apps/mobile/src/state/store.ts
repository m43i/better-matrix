// The store: the engine, the state it folds into, and the two ways to talk to it, `send` (fire
// and forget; a failure becomes a notice) and `run` (await the answer). Views read state with
// `useStore(selector)` and act through the actions in `state/actions`.

import type { Engine, EngineEvent } from '@better-matrix/matrix-native/client';
import type { Command, RequestId } from '@better-matrix/matrix-native/types';

import {
  addNotice,
  apply,
  applyLatestMessages,
  initialState,
  type LatestMessageEvent,
} from './reducer';
import { type RequestEvent, Requests } from './requests';
import { Retries } from './retry';
import type { Settings } from './settings';
import type { State } from './types';

/**
 * Something outside the state that reacts to events: notifications, sounds, saved settings. It
 * hears each event once the state took it in; a run of `LatestMessage` is taken in as a whole
 * first.
 */
export type Effect = (event: EngineEvent, store: Store) => void;

/** How many failed fetches the store remembers, as the desktop's media cache does. */
const FAILURES_KEPT = 512;

export interface SendOptions {
  /** Don't tell the user when it fails: background work (typing, read markers, media). */
  quiet?: boolean;
}

export class Store {
  private state: State;
  private readonly listeners = new Set<() => void>();
  private readonly requests = new Requests();
  private readonly effects: Effect[] = [];
  private notifyQueued = false;
  /** Requests whose failure is shown as a notice. */
  private readonly loud = new Set<RequestId>();
  /** Media and link work under way or fetched well, so each is fetched once (see `want`). */
  private readonly asked = new Set<string>();
  /** The `want` key of each fetch under way, so a failed one may be asked again. */
  private readonly fetching = new Map<RequestId, string>();
  /** Fetches that failed, and when each may be tried again. */
  private readonly retries = new Retries<string>();
  /** What views show and want fetched, by key: the command, and how many views want it. */
  private readonly wanted = new Map<string, { command: Command; views: number }>();
  /** The wake-up that fetches again what is wanted once its wait is over, and when it fires. */
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private retryAt = Infinity;

  constructor(
    readonly engine: Engine,
    settings?: Settings,
  ) {
    this.state = initialState(settings);
    engine.subscribeBatches((events) => this.hearAll(events));
  }

  getState = (): State => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Adds an effect that hears every event after the state took it in. */
  addEffect(effect: Effect): () => void {
    this.effects.push(effect);

    return () => {
      const index = this.effects.indexOf(effect);

      if (index >= 0) this.effects.splice(index, 1);
    };
  }

  /** Changes state on this device (the account on screen, a scope, settings). */
  update(change: (state: State) => State): void {
    this.set(change(this.state));
  }

  /** Sends a command without waiting. Unless `quiet`, a failure is shown as a notice. */
  send(command: Command, options: SendOptions = {}): RequestId {
    const request = this.engine.send(command);

    if (!options.quiet) this.loud.add(request);

    return request;
  }

  /**
   * Sends a command and waits until it finished; rejects with `CommandFailed`. `onEvent` hears
   * the events that answer it on the way.
   */
  run(
    command: Command,
    onEvent?: (event: RequestEvent) => void,
  ): Promise<void> & { request: RequestId } {
    const request = this.engine.send(command);
    const done = Object.assign(this.requests.track(request, onEvent), { request });

    return done;
  }

  /**
   * Wants `command` (media, a link card, a GIF) fetched under `key` until the returned function
   * is called. Each key is fetched once for this run of the app; one that failed waits before it
   * is fetched again (`Retries`: 5 s, doubling, up to 5 minutes). When the wait is over the store
   * fetches it again if anything still wants it, else the next time something does.
   */
  want(key: string, command: Command): () => void {
    const wanted = this.wanted.get(key);

    if (wanted) wanted.views++;
    else this.wanted.set(key, { command, views: 1 });
    this.fetch(key, command);
    this.wakeFor(key);

    let released = false;

    return () => {
      if (released) return;
      released = true;
      const entry = this.wanted.get(key);

      if (entry && --entry.views <= 0) this.wanted.delete(key);
    };
  }

  /** Sends `command` for `key` unless it is under way, fetched already, or waiting to retry. */
  private fetch(key: string, command: Command): void {
    if (this.asked.has(key) || !this.retries.mayTry(key, Date.now())) return;
    this.asked.add(key);
    this.fetching.set(this.send(command, { quiet: true }), key);
  }

  /** A fetch failed: it may be asked again once its wait is over. */
  private fetchFailed(key: string): void {
    this.asked.delete(key);
    this.retries.failed(key, Date.now());
    this.retries.keepLatest(FAILURES_KEPT);
    this.wakeFor(key);
  }

  /** Wakes up when `key`'s wait is over, if it is wanted and waiting. */
  private wakeFor(key: string): void {
    const at = this.retries.waitEnds(key);

    if (at === undefined || at >= this.retryAt || this.asked.has(key) || !this.wanted.has(key))
      return;

    if (this.retryTimer !== undefined) clearTimeout(this.retryTimer);
    this.retryAt = at;
    this.retryTimer = setTimeout(() => this.retryWanted(), Math.max(at - Date.now(), 0));
  }

  /** Fetches again what is wanted and done waiting, then waits for the next wait to end. */
  private retryWanted(): void {
    this.retryTimer = undefined;
    this.retryAt = Infinity;

    for (const [key, { command }] of this.wanted) this.fetch(key, command);

    for (const key of this.wanted.keys()) this.wakeFor(key);
  }

  /**
   * Takes in a burst of events, in order; views redraw once for all of them (see `set`). One
   * event that throws doesn't lose the rest: the first error is thrown once all are in.
   */
  private hearAll(events: readonly EngineEvent[]): void {
    const failures: Array<{ error: unknown }> = [];

    const guarded = (work: () => void) => {
      try {
        work();
      } catch (error) {
        failures.push({ error });
      }
    };

    let i = 0;

    while (i < events.length) {
      // A first sync reports every room's newest message in a row: one copy of the maps for all.
      const run: LatestMessageEvent[] = [];

      for (; i < events.length; i++) {
        const event = events[i];

        if (event?.type !== 'LatestMessage') break;
        run.push(event);
      }

      if (run.length > 0) {
        guarded(() => this.set(applyLatestMessages(this.state, run)));

        for (const event of run) guarded(() => this.heard(event));
      } else {
        const event = events[i++];

        if (event) guarded(() => this.hear(event));
      }
    }

    const first = failures[0];

    if (first) throw first.error;
  }

  private hear(event: EngineEvent): void {
    this.requests.hear(event);

    if (event.type === 'Finished') {
      const key = this.fetching.get(event.data.request);
      this.fetching.delete(event.data.request);

      if (key !== undefined) {
        if ('Err' in event.data.result) this.fetchFailed(key);
        else this.retries.succeeded(key);
      }
    } else if (event.type === 'MediaFailed' && event.data.request !== null) {
      // Fetched, but the file couldn't be written: the fetch finishes well, yet nothing came.
      const key = this.fetching.get(event.data.request);
      this.fetching.delete(event.data.request);

      if (key !== undefined) this.fetchFailed(key);
    }

    let next = apply(this.state, event);

    if (
      event.type === 'Finished' &&
      this.loud.delete(event.data.request) &&
      'Err' in event.data.result
    ) {
      if (event.data.result.Err !== 'cancelled') next = addNotice(next, event.data.result.Err);
    }

    this.set(next);
    this.heard(event);
  }

  /** Tells the effects about an event the state took in. */
  private heard(event: EngineEvent): void {
    for (const effect of this.effects) effect(event, this);
  }

  private set(next: State): void {
    if (next === this.state) return;
    this.state = next;

    // Events come in bursts (a sync, a page of history): views redraw once per burst.
    if (this.notifyQueued) return;
    this.notifyQueued = true;
    queueMicrotask(() => {
      this.notifyQueued = false;

      for (const listener of this.listeners) listener();
    });
  }
}
