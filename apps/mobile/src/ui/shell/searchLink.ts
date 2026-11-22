// A link opened from the search under a tab's title: typed and sent with the search key, or picked
// among the results. Picking empties the search, which takes the results list away, so the jump
// is guarded by the screen hosting the search (`host`), not by the list: the engine's late answer
// still opens the linked message, unless a later jump began or the user left that screen.

import type { AccountId } from '@better-matrix/matrix-native/types';
import type { router as ExpoRouter } from 'expo-router';

import { type Permalink, permalinkUri } from '../../state/permalinks';
import type { Store } from '../../state/store';
import { openPermalink } from '../links/open';

/**
 * Opens `link` for `account` from the search, then empties it (`close`): the jump begins at the
 * pick and goes on while the screen hosting the search is on show (`host`).
 */
export function openSearchLink(
  store: Store,
  link: Permalink,
  account: AccountId | null,
  host: () => boolean,
  close: () => void,
  navigation?: Pick<typeof ExpoRouter, 'navigate' | 'push'>,
): void {
  openPermalink(store, permalinkUri(link), account, host, navigation);
  close();
}
