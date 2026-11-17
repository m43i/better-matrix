// Tells the engine when the app comes to the front and when it leaves (`InFront`). In front, a
// push is never handled outside the engine, so its syncs stop holding the crypto store's lock,
// which writes a lease to the store every 50 ms while held (ADR 0010).

import { AppState, type AppStateStatus } from 'react-native';

import type { Store } from '../state/store';

/** What tells the iOS push extension the app is in front (`app-active`). */
export interface ForegroundMarker {
  /** The app came to the front: the extension leaves pushes to the engine. */
  show(): void;
  /** The app left, and no sync runs without the store's lock any more. */
  hide(): void;
}

/** Where the app's state comes from: React Native's `AppState`, or a test's. */
export interface AppStates {
  readonly currentState: string | null | undefined;
  addEventListener(type: 'change', listener: (state: AppStateStatus) => void): void;
}

/**
 * Follows the app in and out of the front, telling the engine and `marker`. The marker comes
 * down only once the engine answered that its syncs hold the lock again, so the extension never
 * opens the store beside a sync that doesn't.
 */
export function followFront(
  store: Store,
  marker?: ForegroundMarker,
  states: AppStates = AppState,
): void {
  let front: boolean | null = null;

  const follow = (state: string) => {
    const now = state === 'active';

    if (now === front) return;
    front = now;

    if (now) {
      marker?.show();
      store.send({ type: 'InFront', data: true }, { quiet: true });

      return;
    }

    // An engine that failed to answer runs no sync, without the lock or with it.
    const settled = () => {
      if (front === false) marker?.hide();
    };

    store.run({ type: 'InFront', data: false }).then(settled, settled);
  };

  follow(states.currentState ?? 'active');
  states.addEventListener('change', follow);
}
