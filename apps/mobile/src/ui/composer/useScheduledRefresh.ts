// Keeps a conversation's scheduled messages current while it is open: listed when it opens, when
// the app comes back to the foreground, and when the soonest one's send time passes (then again
// every 30 s while the server still lists it), so one already sent leaves the bar
// (`state/schedule.ts` decides when).

import type { AccountId } from '@better-matrix/matrix-native/types';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState } from 'react-native';

import { listScheduled } from '../../state/actions/messages';
import { useAppStore, useStore } from '../../state/react';
import {
  askedAnyway,
  nextRelist,
  type Relisted,
  relistStep,
  soonestScheduled,
  timerDelay,
} from '../../state/schedule';
import { account as accountOf } from '../../state/selectors';

export function useScheduledRefresh(account: AccountId, roomId: string): void {
  const store = useAppStore();
  const due = useStore((s) => soonestScheduled(accountOf(s, account)?.scheduled, roomId));
  const relisted = useRef<Relisted | null>(null);
  /** Changes after each wake, so the next one is set even when the list came back the same. */
  const [wakes, setWakes] = useState(0);

  /** Asks now, which answers for a message already due too. */
  const ask = useCallback(() => {
    const soonest = soonestScheduled(accountOf(store.getState(), account)?.scheduled, roomId);
    relisted.current = askedAnyway(soonest, relisted.current, Date.now());
    listScheduled(store, account);
  }, [store, account, roomId]);

  useEffect(ask, [ask]);

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') ask();
    });

    return () => subscription.remove();
  }, [ask]);

  useEffect(() => {
    const now = Date.now();
    const next = nextRelist(due, relisted.current);

    if (next === null) return;

    const timer = setTimeout(
      () => {
        const step = relistStep(due, relisted.current, Date.now());
        relisted.current = step.relisted;

        if (step.ask) listScheduled(store, account);
        setWakes((n) => n + 1);
      },
      timerDelay(next, now),
    );

    return () => clearTimeout(timer);
  }, [due, wakes, store, account]);
}
