// Opening what a message links to: web pages in the in-app browser (Custom Tabs, Safari View
// Controller), mail and other safe addresses through the system, people and rooms in the app
// (permalinks, ui/links/open.ts).

import type { AccountId } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import * as WebBrowser from 'expo-web-browser';
import { Linking } from 'react-native';

import { t } from '../../i18n';
import { isPermalink } from '../../state/permalinks';
import { addNotice } from '../../state/reducer';
import type { Store } from '../../state/store';
import { routes } from '../../routes';
import { openPermalink } from '../links/open';

/**
 * Opens a link from a message. Only web, mail, FTP and `matrix:` addresses reach here
 * (state/html.ts); a permalink opens in the app, for `account` (the timeline's) when given.
 */
export function openLink(store: Store, url: string, account: AccountId | null = null): void {
  if (isPermalink(url)) {
    openPermalink(store, url, account);

    return;
  }

  const opened = /^https?:/i.test(url) ? WebBrowser.openBrowserAsync(url) : Linking.openURL(url);
  opened.catch((cause: unknown) => {
    store.update((s) => addNotice(s, t('notice.browser', { error: String(cause) })));
  });
}

/** Opens a mention: the person's card, the room when the account is in it, else its link screen. */
export function openMention(
  store: Store,
  account: AccountId,
  roomId: string,
  mention: { kind: 'user' | 'room'; id: string },
): void {
  if (mention.kind === 'user') {
    router.push(routes.member(account, roomId, mention.id));

    return;
  }

  const joined = store
    .getState()
    .accounts.find((a) => a.summary.id === account)
    ?.rooms.some((r) => r.room_id === mention.id);

  if (joined) router.push(routes.room(account, mention.id));
  else openPermalink(store, `https://matrix.to/#/${encodeURIComponent(mention.id)}`, account);
}
