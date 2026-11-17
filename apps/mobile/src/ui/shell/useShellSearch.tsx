// The search under a tab's title, wired for a screen: the bar, the term typed, and the results to
// show in the tab's place while there is one. The search key opens a typed link at once.

import { type ReactNode, useRef, useState } from 'react';

import { t } from '../../i18n';
import { parseTyped, permalinkUri } from '../../state/permalinks';
import { useAppStore } from '../../state/react';
import type { AccountData } from '../../state/types';
import { openPermalink } from '../links/open';
import { SearchResults } from './SearchResults';
import { ShellSearch, type ShellSearchHandle } from './ShellSearch';

export interface ShellSearchView {
  /** The bar: the header's on iOS, a field to put above the tab's content on Android. */
  bar: ReactNode;
  /** The results while something is typed, to show in place of the tab's content. */
  results: ReactNode | null;
}

export function useShellSearch(account: AccountData | undefined): ShellSearchView {
  const store = useAppStore();
  const handle = useRef<ShellSearchHandle>(null);
  const [term, setTerm] = useState('');
  const clear = () => handle.current?.clear();

  const submit = (text: string) => {
    const link = parseTyped(text);

    if (link === null) return;
    clear();
    openPermalink(store, permalinkUri(link), account?.summary.id ?? null);
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
      account && term.trim() !== '' ? (
        <SearchResults account={account} term={term} onPicked={clear} />
      ) : null,
  };
}
