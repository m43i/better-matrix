import type { AccountId } from '@better-matrix/matrix-native/types';
import { useEffect, useState } from 'react';

import { EMPTY_MESSAGE_SEARCH, MessageSearcher } from '../../state/actions/search';
import { useAppStore } from '../../state/react';

export function useMessageSearch(account: AccountId, term: string) {
  const store = useAppStore();
  const [search, setSearch] = useState(EMPTY_MESSAGE_SEARCH);
  useEffect(() => {
    const searcher = new MessageSearcher(store, account, setSearch);
    searcher.type(term);

    return () => searcher.close();
  }, [store, account, term]);

  // An old answer never appears for even one frame after the term changes.
  return search.term === term.trim()
    ? search
    : { ...EMPTY_MESSAGE_SEARCH, status: 'waiting' as const };
}
