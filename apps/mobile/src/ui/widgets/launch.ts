// Opening the widget screen from the app: one of the room's widgets by its ID, a new meeting or
// the integration manager, each with a one-time launch nonce, so a link alone opens nothing. The
// run's bridge token is random too.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { randomUUID } from 'expo-crypto';
import type { Href } from 'expo-router';

import { routes } from '../../routes';
import { Launches } from '../../state/launches';
import type { WidgetTarget } from '../../state/widgets';

/** The launches of the widget screen the app registered. */
export const widgetLaunches = new Launches(randomUUID);

/** A fresh secret for a widget run's bridge script. */
export function bridgeToken(): string {
  return randomUUID();
}

/** Opens the widget screen running `target`, registering its launch. */
export function openWidget(
  router: { push: (href: Href) => void },
  account: AccountId,
  roomId: string,
  target: WidgetTarget,
): void {
  router.push(routes.widget(account, roomId, target, widgetLaunches.register()));
}
