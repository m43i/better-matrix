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

/**
 * A running engine. Each command gets a request ID and ends with exactly one `Finished`
 * event for it; everything else the engine reports reaches every listener.
 */
export class Engine {
  private readonly listeners = new Set<Listener>();

  constructor(private readonly transport: Transport) {}

  /** Sends a command and returns its request ID. */
  send(command: Command): RequestId {
    return this.transport.send(JSON.stringify(command));
  }

  /** Adds a listener; the returned function removes it. */
  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Hands one event to every listener. The transport calls it. */
  emit(event: EngineEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }

  /** Hands one event, as the bridge's JSON, to every listener. */
  emitJson(json: string): void {
    this.emit(JSON.parse(json) as EngineEvent);
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
      sent.push({ request, command: JSON.parse(json) as Command });
      return request;
    },
  });
  return { engine, sent };
}
