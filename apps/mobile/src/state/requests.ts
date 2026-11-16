// Answers to one request: every command ends with exactly one `Finished`, and some report
// events of their own on the way (a discovered homeserver, a URL to open, people found). The
// sender awaits them here, as the desktop routes them by `Purpose`.

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type { RequestId } from '@better-matrix/matrix-native/types';

/** An event that answers one request, by its `request` field. */
export type RequestEvent = Extract<EngineEvent, { data: { request: RequestId } }>;

/** A command the engine finished with an error. */
export class CommandFailed extends Error {
  constructor(
    message: string,
    readonly request: RequestId,
  ) {
    super(message);
    this.name = 'CommandFailed';
  }

  /** Whether it was cancelled rather than failed: nothing to tell the user. */
  get cancelled(): boolean {
    return this.message === 'cancelled';
  }
}

interface Waiting {
  onEvent?: (event: RequestEvent) => void;
  resolve: () => void;
  reject: (error: CommandFailed) => void;
}

export class Requests {
  private readonly waiting = new Map<RequestId, Waiting>();

  /** Waits for `request` to finish; `onEvent` hears the events that answer it meanwhile. */
  track(request: RequestId, onEvent?: (event: RequestEvent) => void): Promise<void> {
    return new Promise((resolve, reject) => {
      this.waiting.set(request, { onEvent, resolve, reject });
    });
  }

  /** Whether anyone waits for `request`. */
  tracks(request: RequestId): boolean {
    return this.waiting.has(request);
  }

  /** Routes one event to whoever waits for its request. Returns whether someone did. */
  hear(event: EngineEvent): boolean {
    if (!('data' in event) || event.data === null || typeof event.data !== 'object' || !('request' in event.data)) {
      return false;
    }
    const request = event.data.request;
    if (request === null || request === undefined) return false;
    const waiting = this.waiting.get(request);
    if (!waiting) return false;
    if (event.type === 'Finished') {
      this.waiting.delete(request);
      if ('Ok' in event.data.result) waiting.resolve();
      else waiting.reject(new CommandFailed(event.data.result.Err, request));
    } else {
      waiting.onEvent?.(event as RequestEvent);
    }
    return true;
  }
}
