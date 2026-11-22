// Message key files leaving and entering the app: an export is written to the cache and handed
// to the system share sheet (save to Files, a cloud drive, another app); an import is picked
// with the system's document picker, which copies it to the cache where the engine can read it.

import type { AccountId } from '@better-matrix/matrix-native/types';
import * as DocumentPicker from 'expo-document-picker';
import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';

import { t } from '../../i18n';
import { exportRoomKeys } from '../../state/actions/security';
import { pathFromUri } from '../../state/outgoing';
import type { Store } from '../../state/store';

/** The export's name, as other Matrix clients name theirs. */
function exportName(now: Date): string {
  return `better-matrix-keys-${now.toISOString().slice(0, 10)}.txt`;
}

/** How long an export stays in the cache: the app it was shared to may read it after the share sheet closed. */
const KEPT_MS = 60 * 60 * 1000;

/**
 * Exports the account's message keys with `passphrase` and opens the share sheet for the file;
 * rejects with why the export failed. Each export gets a folder of its own, named by when it was
 * made, and stays until an export an hour or more later clears it away: on Android the share
 * sheet closes before the receiving app has read the file.
 */
export async function exportAndShare(
  store: Store,
  account: AccountId,
  passphrase: string,
): Promise<void> {
  const now = Date.now();
  const exports = new Directory(Paths.cache, 'keys');

  exports.create({ intermediates: true, idempotent: true });

  for (const entry of exports.list()) {
    if (!(Number.parseInt(entry.name, 10) > now - KEPT_MS)) entry.delete();
  }

  const folder = new Directory(exports, String(now));

  folder.create({ intermediates: true, idempotent: true });
  const file = new File(folder, exportName(new Date(now)));

  await exportRoomKeys(store, account, pathFromUri(file.uri), passphrase);
  await Sharing.shareAsync(file.uri, {
    mimeType: 'text/plain',
    dialogTitle: t('mobile.accounts.export_keys'),
  });
}

/** A key file the user picks, as a plain path the engine reads; `null` when none was picked. */
export async function pickKeyFile(): Promise<string | null> {
  const result = await DocumentPicker.getDocumentAsync({ copyToCacheDirectory: true });
  const asset = result.canceled ? undefined : result.assets[0];

  return asset ? pathFromUri(asset.uri) : null;
}
