// Where the engine's data lives, and the device's saved settings: shared by the app's launch
// (boot.ts) and a push handled without it (push.ts), which must open the same accounts.
//
// On iOS the data lives in the App Group container, so the Notification Service Extension (its
// own process) can open the accounts' stores too (ADR 0010); builds before push kept it in the
// app's documents, and launch moves it once. Android keeps it in the app's documents.

import Constants from 'expo-constants';
import { Directory, File, Paths } from 'expo-file-system';
import { Platform } from 'react-native';

import { parseSettings, type Settings } from '../state/settings';

/** A plain path from an expo-file-system URI. */
export function pathOf(uri: string): string {
  return decodeURI(uri.replace(/^file:\/\//, ''));
}

/** The App Group the app and its Notification Service Extension share (iOS), from the app config. */
export function appGroup(): string | null {
  const push = (Constants.expoConfig?.extra as { push?: { appGroup?: unknown } } | undefined)?.push;
  return typeof push?.appGroup === 'string' && push.appGroup ? push.appGroup : null;
}

/** The shared container of the App Group, when this is iOS and the build has one. */
function groupContainer(): Directory | null {
  if (Platform.OS !== 'ios') return null;
  const group = appGroup();
  if (!group) return null;
  try {
    return Paths.appleSharedContainers[group] ?? null;
  } catch {
    return null;
  }
}

/**
 * The engine's data directory: `matrix` in the App Group container on iOS (moved there from the
 * documents on the first launch that has one), else in the documents. Created if missing.
 */
export function dataDirectory(): Directory {
  const documents = new Directory(Paths.document, 'matrix');
  const container = groupContainer();
  const data = container ? new Directory(container, 'matrix') : documents;
  if (container && !data.exists && documents.exists) {
    try {
      // The engine isn't running yet, so nothing has the stores open. The account list's store
      // paths follow the move (matrix-core rebases them under the data directory).
      documents.moveSync(data);
    } catch {
      // The data stays where it was; push decrypts nothing until it moves.
      return ensure(documents);
    }
  }
  return ensure(data);
}

function ensure(directory: Directory): Directory {
  directory.create({ intermediates: true, idempotent: true });
  return directory;
}

const settingsFile = () => new File(Paths.document, 'settings.json');

/** The device's saved settings; defaults when there are none or they don't read. */
export function readSettings(): Settings {
  try {
    const file = settingsFile();
    return parseSettings(file.exists ? file.textSync() : null);
  } catch {
    return parseSettings(null);
  }
}

/** Saves settings off the critical path; a failed save keeps the old file. */
export function saveSettings(settings: Settings): void {
  try {
    settingsFile().write(JSON.stringify(settings));
  } catch {
    // Settings are a convenience: the next change tries again.
  }
}
