// Links into the app, before the router sees them. The OAuth redirect belongs to the browser
// session waiting for it (ui/accounts/browser.ts), not to a screen: the router stays where it is.
// Permalinks (`matrix:` URIs, matrix.to links on Android) wait in the link inbox until an account
// can open them (ui/links/LinkOpener.tsx), so the router stays where it is for them too.

import { demoScene } from '../demo/link';
import { isPermalink } from '../state/permalinks';
import { isOAuthRedirect } from '../ui/accounts/redirect';
import { deliverLink } from '../ui/links/inbox';

export function redirectSystemPath({
  path,
  initial,
}: {
  path: string;
  initial: boolean;
}): string | null {
  // Demo links pick a scene (src/app/_layout.tsx), which opens its own screens.
  if (demoScene(path)) return initial ? '/' : null;

  if (isPermalink(path)) {
    deliverLink(path);

    return initial ? '/' : null;
  }

  if (!isOAuthRedirect(path)) return path;

  // Opened by the redirect alone (the app was gone meanwhile), the sign-in is gone too: start over.
  return initial ? '/' : null;
}
