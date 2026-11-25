// The user's own GIPHY API key, for the GIF picker: kept in the device's secure store (the iOS
// keychain, Android's keystore) rather than with the settings. Read at launch into state; saved
// and removed through `saveGiphyKey` (state/actions/stickers.ts), which waits for the secure
// store before the app says the key is kept or gone.

import * as SecureStore from 'expo-secure-store';

import { type KeyVault, showGiphyKey, keepGiphyKeyIn } from '../state/actions/stickers';
import type { Store } from '../state/store';

const KEY = 'giphy-api-key';

const OPTIONS: SecureStore.SecureStoreOptions = {
  keychainService: 'chat.bettermatrix.mobile.giphy',
  keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
};

/** The secure store, as the key's vault. */
const SECURE_VAULT: KeyVault = {
  save: (key) => SecureStore.setItemAsync(KEY, key, OPTIONS),
  remove: () => SecureStore.deleteItemAsync(KEY, OPTIONS),
};

/** Reads the kept key into state, and keeps changes in the secure store from now on. */
export function installGiphyKey(store: Store): void {
  keepGiphyKeyIn(SECURE_VAULT);
  void SecureStore.getItemAsync(KEY, OPTIONS).then(
    (key) => showGiphyKey(store, key),
    () => showGiphyKey(store, null),
  );
}
