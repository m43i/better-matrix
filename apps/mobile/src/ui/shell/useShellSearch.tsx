// The search under a tab's title, wired for a screen: the bar, the term typed, and the results to
// show in the tab's place while there is one. The search key opens a typed link at once.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { type ReactNode, useRef, useState } from 'react';

import { t } from '../../i18n';
import { parseTyped, permalinkUri } from '../../state/permalinks';
import { useAppStore, useStore } from '../../state/react';
import { account as accountOf } from '../../state/selectors';
import { openPermalink } from '../links/open';
import { useShown } from '../useShown';
import { SearchResults } from './SearchResults';
import { ShellSearch, type ShellSearchHandle } from './ShellSearch';

export interface ShellSearchView {
  /** The bar: the header's on iOS, a field to put above the tab's content on Android. */
  bar: ReactNode;
  /** The results while something is typed, to show in place of the tab's content. */
  results: ReactNode | null;
}

/** The results for an account, which follow all of it, so the tab itself needn't. */
function AccountSearchResults({
  id,
  term,
  onPicked,
}: {
  id: AccountId;
  term: string;
  onPicked: () => void;
}) {
  const account = useStore((s) => accountOf(s, id));

  return account ? <SearchResults account={account} term={term} onPicked={onPicked} /> : null;
}

export function useShellSearch(account: AccountId | undefined): ShellSearchView {
  const store = useAppStore();
  const handle = useRef<ShellSearchHandle>(null);
  const [term, setTerm] = useState('');
  const clear = () => handle.current?.clear();
  const shown = useShown();

  const submit = (text: string) => {
    const link = parseTyped(text);

    if (link === null) return;
    clear();
    openPermalink(store, permalinkUri(link), account ?? null, () => shown.current);
  };

  const bar = (
    <ShellSearch
      ref={handle}
      placeholder={t('mobile.shell.search')}
      onChange={setTerm}
      onSubmit={submit}
    />
  );

  return {
    bar,
    results:
      account !== undefined && term.trim() !== '' ? (
        // One search per account: switching accounts starts its directory search afresh.
        <AccountSearchResults key={account} id={account} term={term} onPicked={clear} />
      ) : null,
  };
}
