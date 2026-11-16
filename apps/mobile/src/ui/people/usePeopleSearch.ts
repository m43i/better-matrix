// One people search for a screen's search field: suggestions on open, the directory as typed.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { useCallback, useEffect, useRef, useState } from 'react';

import { PeopleSearcher } from '../../state/actions/people';
import { EMPTY_SEARCH, type PeopleSearch } from '../../state/people';
import { useAppStore } from '../../state/react';

/** The search as it stands, and the function the field calls with its text. */
export function usePeopleSearch(account: AccountId | null): [PeopleSearch, (text: string) => void] {
  const store = useAppStore();
  const [search, setSearch] = useState<PeopleSearch>(EMPTY_SEARCH);
  const searcher = useRef<PeopleSearcher | null>(null);
  useEffect(() => {
    if (account === null) return;
    const next = new PeopleSearcher(store, account, setSearch);
    searcher.current = next;
    next.suggest();

    return () => {
      next.close();
      searcher.current = null;
    };
  }, [store, account]);
  const type = useCallback((text: string) => searcher.current?.type(text), []);

  return [search, type];
}
