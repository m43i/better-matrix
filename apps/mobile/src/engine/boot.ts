// Starts the engine and the store at launch: settings first (read before the first frame, as
// the desktop does), then the native engine, which restores saved accounts at once.

import { startNativeEngine } from '@better-matrix/matrix-native/native';
import { Directory, Paths } from 'expo-file-system';

import { setLanguage } from '../i18n';
import { installAccountEffects } from '../state/actions/accounts';
import { Store } from '../state/store';
import { CLIENT_NAME, CLIENT_URI, OAUTH_REDIRECT } from './identity';
import { installNotifications } from './notifications';
import { dataDirectory, pathOf, readSettings, saveSettings } from './paths';
import { installPush } from './push';
import { secureStore } from './secrets';

export { pathOf, saveSettings } from './paths';

export function boot(): Store {
  const settings = readSettings();
  setLanguage(settings.language);
  // In the App Group container on iOS, so pushes can be read while the app isn't running.
  const data = dataDirectory();
  const cache = new Directory(Paths.cache, 'matrix');
  cache.create({ intermediates: true, idempotent: true });

  const engine = startNativeEngine(
    {
      dataDir: pathOf(data.uri),
      cacheDir: pathOf(cache.uri),
      clientName: CLIENT_NAME,
      clientUri: CLIENT_URI,
      oauthRedirect: OAUTH_REDIRECT,
    },
    secureStore,
  );

  const store = new Store(engine, settings);
  installNotifications(store);
  installAccountEffects(store);
  installPush(store, data);
  let saved = settings;
  store.subscribe(() => {
    const next = store.getState().settings;

    if (next === saved) return;

    if (next.language !== saved.language) setLanguage(next.language);
    saved = next;
    saveSettings(next);
  });

  return store;
}
