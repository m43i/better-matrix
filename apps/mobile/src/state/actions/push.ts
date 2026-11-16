// Push on this device (engine/push.ts keeps the pushers in step with these settings).

import type { AccountId } from '@better-matrix/matrix-native/types';

import type { Settings } from '../settings';
import type { Store } from '../store';
import { updateSettings } from './core';

/** "Notifications on this device": pushes while the app isn't running, for every account. */
export function setPushEnabled(store: Store, on: boolean): void {
  updateSettings(store, { pushEnabled: on });
}

/** The accounts turned off for push after turning `account` on or off. */
export function pushOffWith(off: readonly AccountId[], account: AccountId, on: boolean): string[] {
  const others = off.filter((id) => id !== account);

  return on ? others : [...others, account];
}

/** Whether `account` gets pushes on this device. */
export function accountPushOn(
  settings: Pick<Settings, 'pushEnabled' | 'pushOff'>,
  account: AccountId,
): boolean {
  return settings.pushEnabled && !settings.pushOff.includes(account);
}

/** One account's pushes on this device, on or off. */
export function setAccountPush(store: Store, account: AccountId, on: boolean): void {
  updateSettings(store, { pushOff: pushOffWith(store.getState().settings.pushOff, account, on) });
}
