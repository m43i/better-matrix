// React bindings: one store per app, read through selectors.

import { createContext, type ReactNode, useCallback, useContext, useRef, useSyncExternalStore } from 'react';

import type { Store } from './store';
import type { State } from './types';

const StoreContext = createContext<Store | null>(null);

export function StoreProvider({ store, children }: { store: Store; children: ReactNode }) {
  return <StoreContext.Provider value={store}>{children}</StoreContext.Provider>;
}

/** The app's store, for actions. */
export function useAppStore(): Store {
  const store = useContext(StoreContext);
  if (!store) throw new Error('useAppStore needs a StoreProvider');
  return store;
}

/** What `useStore` read last: the state and selector it read with, and the value. */
export interface Selected<S, T> {
  state: S;
  selector: (state: S) => T;
  value: T;
}

/**
 * The value `selector` reads from `state`, reusing the one read last while neither the state nor
 * the selector changed, and keeping the last value's reference while `equal` calls it the same.
 * A new selector (one closing over new props) reads again even when the state did not change.
 */
export function selectFrom<S, T>(
  last: Selected<S, T> | null,
  state: S,
  selector: (state: S) => T,
  equal: (a: T, b: T) => boolean,
): Selected<S, T> {
  if (last && last.state === state && last.selector === selector) return last;
  const value = selector(state);
  if (last && equal(last.value, value)) return { state, selector, value: last.value };
  return { state, selector, value };
}

/**
 * A slice of the state; the view redraws when it changes. `equal` decides "changed" (by
 * reference unless given), so selectors that build new arrays or objects should pass `shallow`.
 */
export function useStore<T>(selector: (state: State) => T, equal: (a: T, b: T) => boolean = Object.is): T {
  const store = useAppStore();
  const last = useRef<Selected<State, T> | null>(null);
  const select = useCallback(() => {
    last.current = selectFrom(last.current, store.getState(), selector, equal);
    return last.current.value;
  }, [store, selector, equal]);
  return useSyncExternalStore(store.subscribe, select, select);
}

/** Equal when both are arrays or objects with the same items or fields by reference. */
export function shallow<T>(a: T, b: T): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => Object.is((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}
