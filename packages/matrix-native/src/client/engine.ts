// The engine as the app sees it: typed commands in, typed events out (ADR 0010). The native
// bridge is one transport; a detached engine is another, for tests and previews.

import type { BridgeEvent, Command, Event, RequestId } from '../generated/types';

/** Everything the app hears from the engine: its own events, and the bridge's file events. */
export type EngineEvent = Event | BridgeEvent;

/** What carries commands to an engine. */
export interface Transport {
  /** Sends one command, as JSON, and returns its request ID. */
  send(command: string): number;
}

/** Hears every event. */
export type Listener = (event: EngineEvent) => void;

/** Hears every event, as the batches they arrive in (in order; never empty). */
export type BatchListener = (events: readonly EngineEvent[]) => void;

/**
 * A running engine. Each command gets a request ID and ends with exactly one `Finished`
 * event for it; everything else the engine reports reaches every listener.
 */
export class Engine {
  private readonly listeners = new Set<BatchListener>();

  constructor(private readonly transport: Transport) {}

  /** Sends a command and returns its request ID. */
  send(command: Command): RequestId {
    return this.transport.send(JSON.stringify(command));
  }

  /**
   * Adds a listener that hears events one by one; the returned function removes it. Listeners
   * take turns per batch: each hears a whole batch, in order, before the next listener hears any
   * of it, so with several listeners one may hear a later event before another hears an earlier.
   */
  subscribe(listener: Listener): () => void {
    return this.subscribeBatches((events) => {
      for (const event of events) listener(event);
    });
  }

  /**
   * Adds a listener that hears a burst of events at once (a sync, a page of history), so it can
   * take them in together; the returned function removes it. Listeners take turns per batch:
   * each hears the whole batch before the next listener hears any of it.
   */
  subscribeBatches(listener: BatchListener): () => void {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Hands one event to every listener. The transport calls it. */
  emit(event: EngineEvent): void {
    this.emitAll([event]);
  }

  /** Hands events, in order, to every listener. */
  emitAll(events: readonly EngineEvent[]): void {
    if (events.length === 0) return;

    for (const listener of this.listeners) {
      listener(events);
    }
  }

  /**
   * Hands the bridge's JSON to every listener: one event, or an array of the events that were
   * waiting together (`matrix_ffi::events::next_batch`).
   */
  emitJson(json: string): void {
    // SAFETY: the Rust bridge serialized Event or BridgeEvent, alone or in an array, from the same types generated into this package.
    const parsed = JSON.parse(json) as EngineEvent | EngineEvent[];

    this.emitAll(Array.isArray(parsed) ? parsed : [parsed]);
  }
}

/** An engine with nothing behind it: commands are recorded, events come from the caller. */
export interface DetachedEngine {
  engine: Engine;
  /** Every command sent, in order, with its request ID. */
  sent: Array<{ request: RequestId; command: Command }>;
}

/** An engine for tests and previews, the counterpart of the desktop's detached handle. */
export function detachedEngine(): DetachedEngine {
  const sent: DetachedEngine['sent'] = [];
  let next = 1;

  const engine = new Engine({
    send(json) {
      const request = next++;
      // SAFETY: Engine.send serialized the typed Command immediately before this detached transport received it.
      sent.push({ request, command: JSON.parse(json) as Command });

      return request;
    },
  });

  return { engine, sent };
}
