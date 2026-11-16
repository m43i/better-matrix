// How the app introduces itself to OAuth servers. The redirect scheme must be the client URI's
// host reversed (MSC2966), and is registered in app.json's `scheme`. The desktop's CLIENT_URI is
// the same placeholder; both change once the project has a site.
export const CLIENT_NAME = 'Better Matrix';

export const CLIENT_URI = 'https://example.org/better-matrix';

export const OAUTH_REDIRECT = 'org.example.bettermatrix:/oauth';
