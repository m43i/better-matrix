// Opening a permalink, from a message or from the system: a room an account has joined opens at
// once (at the linked message), anything else on the link screen (app/link.tsx).

import type { AccountId, TimelineKey } from '@better-matrix/matrix-native/types';
import type { router as ExpoRouter } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { setActive, setScope } from '../../state/actions/core';
import { locateMessage } from '../../state/actions/messages';
import { type LinkRoute, routeLink } from '../../state/linkRoute';
import { parsePermalink } from '../../state/permalinks';
import { addNotice } from '../../state/reducer';
import type { Store } from '../../state/store';

function appRouter(): typeof ExpoRouter {
  // SAFETY: the literal require resolves Expo Router's typed module; tests inject navigation before this boundary.
  const { router } = require('expo-router') as typeof import('expo-router');

  return router;
}

/** Opens `link`, for `preferred` (the account it was tapped in) when given. */
export function openPermalink(
  store: Store,
  link: string,
  preferred: AccountId | null = null,
): void {
  const route = routeLink(store.getState(), parsePermalink(link), preferred);

  switch (route.type) {
    case 'invalid':
      store.update((s) => addNotice(s, t('link.invalid')));

      return;
    case 'room':
    case 'space':
      openJoined(store, route);

      return;
    case 'screen':
      appRouter().push(routes.link(link, route.account ?? undefined));
  }
}

/**
 * Opens a room an account has joined, or shows a joined space as the account's scope on Home. A
 * link to a message asks the engine which timeline shows it first, so a reply in a thread opens
 * in its thread; the room's timeline when the engine can't tell.
 */
export function openJoined(
  store: Store,
  route: Extract<LinkRoute, { type: 'room' | 'space' }>,
  navigation: Pick<typeof ExpoRouter, 'navigate' | 'push'> = appRouter(),
): void {
  if (store.getState().active !== route.account) setActive(store, route.account);

  if (route.type === 'space') {
    setScope(store, route.account, { space: route.spaceId });
    navigation.navigate(routes.home());

    return;
  }

  const { account, roomId, eventId } = route;

  if (eventId === null) {
    navigation.push(routes.room(account, roomId));

    return;
  }

  locateMessage(store, account, roomId, eventId).then(
    (key) => navigation.push(messageRoute(account, roomId, eventId, key.thread)),
    () => navigation.push(routes.room(account, roomId, eventId)),
  );
}

/**
 * Opens a message the timeline `current` didn't find by paging back (a quote of a reply in a
 * thread, or of a room message from inside a thread): the engine says which timeline shows it,
 * and that one opens at it, as a permalink's message does. Resolves whether it opened: `false`
 * when the message would show in `current` (it is gone) or the engine can't find it.
 */
export async function openElsewhere(
  store: Store,
  account: AccountId,
  current: TimelineKey,
  eventId: string,
  navigation: Pick<typeof ExpoRouter, 'push'> = appRouter(),
): Promise<boolean> {
  let located: TimelineKey;

  try {
    located = await locateMessage(store, account, current.room_id, eventId);
  } catch {
    return false;
  }

  const thread = located.thread !== null && located.thread !== eventId ? located.thread : null;

  if (thread === current.thread) return false;
  navigation.push(messageRoute(account, current.room_id, eventId, thread));

  return true;
}

/** Where a message shows: its thread (the root itself shows in the room), else the room. */
export function messageRoute(
  account: AccountId,
  roomId: string,
  eventId: string,
  thread: string | null,
): ReturnType<typeof routes.room> {
  return thread !== null && thread !== eventId
    ? routes.thread(account, roomId, thread, eventId)
    : routes.room(account, roomId, eventId);
}
