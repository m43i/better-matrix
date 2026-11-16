// The engine's secret store on phones: the iOS keychain and Android's keystore, through
// expo-secure-store. Its keys allow only letters, digits, `.`, `-` and `_`, so others are escaped.

import type { SecretStore } from '@better-matrix/matrix-native/native';
import * as SecureStore from 'expo-secure-store';

/** A key SecureStore accepts, one-to-one with the engine's: `_` and disallowed characters escape as `_xx`. */
export function storeKey(key: string): string {
  return [...new TextEncoder().encode(key)]
    .map((byte) => {
      const c = String.fromCharCode(byte);

      return /[A-Za-z0-9.-]/.test(c) ? c : `_${byte.toString(16).padStart(2, '0')}`;
    })
    .join('');
}

const OPTIONS: SecureStore.SecureStoreOptions = {
  keychainService: 'chat.bettermatrix.mobile',
  // Restoring at launch and syncing in the background need them while the phone is locked.
  keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
};

export const secureStore: SecretStore = {
  get: (key) => SecureStore.getItem(storeKey(key), OPTIONS),
  set: (key, value) => SecureStore.setItem(storeKey(key), value, OPTIONS),
  delete: (key) => {
    // expo-secure-store has no synchronous delete; the engine never reads a key right after
    // deleting it, so finishing in the background is enough.
    void SecureStore.deleteItemAsync(storeKey(key), OPTIONS);
  },
};
