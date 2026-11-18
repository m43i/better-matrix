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
import type { Settings } from './settings';
import type { State } from './types';

/**
 * Something outside the state that reacts to events: notifications, sounds, saved settings. It
 * hears each event once the state took it in; a run of `LatestMessage` is taken in as a whole
 * first.
 */
export type Effect = (event: EngineEvent, store: Store) => void;

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
  /** Media and link work asked for already, so each is fetched once (see `fetchOnce`). */
  private readonly asked = new Set<string>();
  /** The `fetchOnce` key of each fetch under way, so a failed one may be asked again. */
  private readonly fetching = new Map<RequestId, string>();

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

  /** Sends `command` once per `key` for this run of the app (media, link cards, GIFs). */
  fetchOnce(key: string, command: Command): void {
    if (this.asked.has(key)) return;
    this.asked.add(key);
    this.fetching.set(this.send(command, { quiet: true }), key);
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

      // A failed fetch may be asked again, the next time a view needs it.
      if (key !== undefined && 'Err' in event.data.result) this.asked.delete(key);
    } else if (event.type === 'MediaFailed' && event.data.request !== null) {
      // Fetched, but the file couldn't be written: the fetch finishes well, yet nothing came.
      const key = this.fetching.get(event.data.request);

      if (key !== undefined) this.asked.delete(key);
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
