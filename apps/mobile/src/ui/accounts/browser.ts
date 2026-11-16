// The browser, for signing in (a session that returns to the app) and for the homeserver's
// account pages (an in-app browser the user closes).

import * as Linking from 'expo-linking';
import * as WebBrowser from 'expo-web-browser';

import type { AuthBrowser } from '../../state/actions/accounts';
import { t } from '../../i18n';
import { addNotice } from '../../state/reducer';
import type { Store } from '../../state/store';

/**
 * How long a closed browser waits for the redirect: on Android the app coming back to the front
 * and the redirect's link are separate events, and either may come first.
 */
const REDIRECT_GRACE_MS = 1500;

/**
 * Opens a sign-in page in the system's authentication session (`ASWebAuthenticationSession`,
 * Custom Tabs); the URL it returned to the app with, or `null` when it was closed without one.
 */
export const authBrowser: AuthBrowser = async (url, redirect) => {
  let redirected: string | null = null;
  let heard: (() => void) | null = null;
  const subscription = Linking.addEventListener('url', (event) => {
    if (!event.url.startsWith(redirect)) return;
    redirected = event.url;
    heard?.();
  });
  try {
    const result = await WebBrowser.openAuthSessionAsync(url, redirect);
    if (result.type === 'success') return result.url;
    if (redirected === null) {
      await new Promise<void>((resolve) => {
        heard = resolve;
        setTimeout(resolve, REDIRECT_GRACE_MS);
      });
    }
    return redirected;
  } finally {
    subscription.remove();
  }
};

/** Opens a page in the in-app browser; a failure becomes a notice. */
export function openInBrowser(store: Store, url: string): void {
  WebBrowser.openBrowserAsync(url).catch((error: unknown) => {
    store.update((s) => addNotice(s, t('notice.browser', { error: String(error) })));
  });
}
