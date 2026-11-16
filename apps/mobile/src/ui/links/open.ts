// Opening a permalink, from a message or from the system: a room an account has joined opens at
// once (at the linked message), anything else on the link screen (app/link.tsx).

import type { AccountId } from '@better-matrix/matrix-native/types';
import type { router as ExpoRouter } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { setActive, setScope } from '../../state/actions/core';
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

/** Opens a room an account has joined, or shows a joined space as the account's scope on Home. */
export function openJoined(
  store: Store,
  route: Extract<LinkRoute, { type: 'room' | 'space' }>,
  navigation: Pick<typeof ExpoRouter, 'navigate' | 'push'> = appRouter(),
): void {
  if (store.getState().active !== route.account) setActive(store, route.account);

  if (route.type === 'space') {
    setScope(store, route.account, { space: route.spaceId });
    navigation.navigate(routes.home());
  } else {
    navigation.push(routes.room(route.account, route.roomId, route.eventId ?? undefined));
  }
}
