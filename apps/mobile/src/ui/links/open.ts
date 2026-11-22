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
import { type Jump, jumps } from './jumps';

/** The app's jumps: a new one supersedes any whose answer hasn't come yet. */
const JUMPS = jumps();

/**
 * Begins a jump at the tap that asks for it, from a screen on show while `shown` says so: any
 * earlier jump's answer no longer navigates, and this one's only while it is the latest.
 */
export function beginJump(shown?: () => boolean): Jump {
  return JUMPS.begin(shown);
}

function appRouter(): typeof ExpoRouter {
  // SAFETY: the literal require resolves Expo Router's typed module; tests inject navigation before this boundary.
  const { router } = require('expo-router') as typeof import('expo-router');

  return router;
}

/**
 * Opens `link`, for `preferred` (the account it was tapped in) when given, from a screen on show
 * while `shown` says so.
 */
export function openPermalink(
  store: Store,
  link: string,
  preferred: AccountId | null = null,
  shown?: () => boolean,
  navigation: Pick<typeof ExpoRouter, 'navigate' | 'push'> = appRouter(),
): void {
  const route = routeLink(store.getState(), parsePermalink(link), preferred);

  switch (route.type) {
    case 'invalid':
      store.update((s) => addNotice(s, t('link.invalid')));

      return;
    case 'room':
    case 'space':
      openJoined(store, route, navigation, shown);

      return;
    case 'screen':
      // Where the user goes now, no earlier jump's answer takes them away.
      JUMPS.begin();
      navigation.push(routes.link(link, route.account ?? undefined));
  }
}

/**
 * Opens a room an account has joined, or shows a joined space as the account's scope on Home. A
 * link to a message asks the engine which timeline shows it first, so a reply in a thread opens
 * in its thread; the room's timeline when the engine can't tell. The answer opens it only while
 * this is the latest jump and the screen that asked is on show (`shown`).
 */
export function openJoined(
  store: Store,
  route: Extract<LinkRoute, { type: 'room' | 'space' }>,
  navigation: Pick<typeof ExpoRouter, 'navigate' | 'push'> = appRouter(),
  shown?: () => boolean,
): void {
  const wanted = JUMPS.begin(shown);

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
    (key) => {
      if (wanted()) navigation.push(messageRoute(account, roomId, eventId, key.thread));
    },
    () => {
      if (wanted()) navigation.push(routes.room(account, roomId, eventId));
    },
  );
}

/**
 * What became of a jump to a message elsewhere: it opened, it shows in the timeline that asked
 * after all (further back than it paged: look on), the engine can't find it, or the answer came
 * once the jump was no longer wanted.
 */
export type Elsewhere = 'opened' | 'here' | 'missing' | 'stale';

/**
 * Opens a message the timeline `current` didn't find by paging back (a quote of a reply in a
 * thread, or of a room message from inside a thread): the engine says which timeline shows it,
 * and that one opens at it, as a permalink's message does, while `wanted` says so: the jump
 * begun at the quote's tap (`beginJump`, with its screen's guard), so paging back first doesn't
 * make it newer than a jump tapped since.
 */
export async function openElsewhere(
  store: Store,
  account: AccountId,
  current: TimelineKey,
  eventId: string,
  wanted: Jump,
  navigation: Pick<typeof ExpoRouter, 'push'> = appRouter(),
): Promise<Elsewhere> {
  if (!wanted()) return 'stale';
  let located: TimelineKey;

  try {
    located = await locateMessage(store, account, current.room_id, eventId);
  } catch {
    return wanted() ? 'missing' : 'stale';
  }

  if (!wanted()) return 'stale';
  const thread = located.thread !== null && located.thread !== eventId ? located.thread : null;

  if (thread === current.thread) return 'here';
  navigation.push(messageRoute(account, current.room_id, eventId, thread));

  return 'opened';
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
