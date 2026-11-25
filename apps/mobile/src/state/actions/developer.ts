// Developer tools and widgets (the desktop's `state/developer.rs`): an event's source, and a
// room's widgets, web apps pinned to it (running them is `state/actions/widgets.ts`).

import type { AccountId, EventSource } from '@better-matrix/matrix-native/types';

import { updateAccount } from '../reducer';
import type { Store } from '../store';
import { attempt } from './people';

/** An event's JSON, as this device reads it and, encrypted, as it was sent. */
export async function viewSource(
  store: Store,
  account: AccountId,
  roomId: string,
  eventId: string,
): Promise<EventSource> {
  let source: EventSource | null = null;
  await store.run(
    { type: 'ViewSource', data: { account, room_id: roomId, event_id: eventId } },
    (event) => {
      if (event.type === 'EventSource') source = event.data.source;
    },
  );

  if (source === null) throw new Error('no source');

  return source;
}

/** Lists a room's widgets into `AccountState.widgets`; failing, the list stops loading, empty. */
export function listWidgets(store: Store, account: AccountId, roomId: string): void {
  attempt(store, { type: 'ListWidgets', data: { account, room_id: roomId } }, 'notice.side').catch(
    () => {
      store.update((s) =>
        updateAccount(s, account, (a) => ({
          ...a,
          widgets: {
            ...a.widgets,
            [roomId]: a.widgets[roomId] ?? { widgets: [], can_add: false },
          },
        })),
      );
    },
  );
}

/** Pins a web page to the room; the room's widgets are listed again once it shows. */
export function addWidget(
  store: Store,
  account: AccountId,
  roomId: string,
  name: string,
  url: string,
): Promise<void> {
  const command = { account, room_id: roomId, name: name.trim(), url: url.trim() };

  return attempt(store, { type: 'AddWidget', data: command }, 'notice.widget');
}

/** Takes a widget off the room, for everyone in it. */
export function removeWidget(
  store: Store,
  account: AccountId,
  roomId: string,
  widgetId: string,
): void {
  const command = { account, room_id: roomId, widget_id: widgetId };
  attempt(store, { type: 'RemoveWidget', data: command }, 'notice.widget').catch(() => {});
}
