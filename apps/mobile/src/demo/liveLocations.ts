// The demo homeserver aggregates each device update onto its original room beacon.
import type { Engine } from '@better-matrix/matrix-native/client';
import type {
  Command,
  LiveDuration,
  Message,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';
import { mapOf } from '../state/geo';
import { applyDiffs, roomKey } from '../state/timeline';

const HOUR_MS = 60 * 60 * 1000;

/** How long a beacon runs, as the engine's `LiveDuration::timeout` sets it. */
export const LIVE_DURATION_MS: Record<LiveDuration, number> = {
  Hour: HOUR_MS,
  Day: 24 * HOUR_MS,
  // Until stopped still names an end, 24 days on: other clients' timers take no longer.
  UntilStopped: 24 * 24 * HOUR_MS,
};

export function demoLiveLocations(engine: Engine, finish: (request: number) => void) {
  const rooms = new Map<string, TimelineEntry[]>();

  const shares = new Map<
    number,
    { account: string; room: string; timer: ReturnType<typeof setTimeout> }
  >();

  const key = (account: string, room: string) => JSON.stringify([account, room]);
  engine.subscribe((event) => {
    if (event.type !== 'TimelineChanged' || event.data.key.thread) return;
    const room = key(event.data.account, event.data.key.room_id);
    rooms.set(room, applyDiffs(rooms.get(room) ?? [], event.data.diffs));
  });

  function update(request: number, uri?: string) {
    const share = shares.get(request);

    if (!share) return;
    const entries = rooms.get(key(share.account, share.room)) ?? [];
    const index = entries.findIndex((entry) => entry.id === `$beacon-${request}`);
    const entry = entries[index];

    if (entry?.kind.type === 'Message' && entry.kind.data.content.type === 'Location') {
      const message = entry.kind.data;
      const location = entry.kind.data.content.data;

      const data = uri
        ? { ...location, geo_uri: uri, map: mapOf(uri) }
        : { ...location, live_until: 0 };

      engine.emit({
        type: 'TimelineChanged',
        data: {
          account: share.account,
          key: roomKey(share.room),
          diffs: [
            {
              type: 'Set',
              data: {
                index,
                entry: {
                  id: entry.id,
                  kind: {
                    type: 'Message',
                    data: { ...message, content: { type: 'Location', data } },
                  },
                },
              },
            },
          ],
        },
      });
    }

    if (!uri) {
      clearTimeout(share.timer);
      shares.delete(request);
      finish(request);
    }
  }

  return (command: Command, request: number) => {
    if (command.type === 'ShareLiveLocation') {
      const { account, room_id: room, geo_uri } = command.data;
      const now = Date.now();
      const duration = LIVE_DURATION_MS[command.data.duration];

      const message: Message = {
        event_id: `$beacon-${request}`,
        sender_id: account,
        sender_name: 'Ada Lovelace',
        sender_avatar: null,
        content: {
          type: 'Location',
          data: { geo_uri, description: null, map: mapOf(geo_uri), live_until: now + duration },
        },
        edited: false,
        editable: false,
        pending: false,
        timestamp_ms: now,
        reactions: [],
        reply_to: null,
        thread: null,
        highlighted: false,
      };

      shares.set(request, { account, room, timer: setTimeout(() => update(request), duration) });
      engine.emit({
        type: 'TimelineChanged',
        data: {
          account,
          key: roomKey(room),
          diffs: [
            {
              type: 'PushBack',
              data: { id: message.event_id, kind: { type: 'Message', data: message } },
            },
          ],
        },
      });
      engine.emit({ type: 'LiveLocationStarted', data: { request, event_id: message.event_id } });

      return true;
    }

    if (command.type === 'Cancel' && shares.has(command.data)) {
      update(command.data);
      finish(request);

      return true;
    }

    if (
      command.type === 'Answer' &&
      command.data.answer.type === 'LocationFix' &&
      shares.has(command.data.request)
    ) {
      update(command.data.request, command.data.answer.data);
      finish(request);

      return true;
    }

    return false;
  };
}
