// Recognising the OAuth redirect (`org.example.bettermatrix:/oauth?…`, ADR 0010) among the links
// that open the app: it belongs to the browser session waiting for it, not to a screen.

import { OAUTH_REDIRECT } from '../../engine/identity';

/** Whether `path` is the OAuth redirect, as the scheme's URL or the path the router makes of it. */
export function isOAuthRedirect(path: string): boolean {
  // `org.example.bettermatrix:/oauth` → `/oauth`
  const redirectPath = OAUTH_REDIRECT.slice(OAUTH_REDIRECT.indexOf(':') + 1);
  const ends = (rest: string) => /^([/?#]|$)/.test(rest);

  if (path.startsWith(OAUTH_REDIRECT)) return ends(path.slice(OAUTH_REDIRECT.length));
  const bare = `/${path.replace(/^\/+/, '')}`;

  return bare.startsWith(redirectPath) && ends(bare.slice(redirectPath.length));
}
