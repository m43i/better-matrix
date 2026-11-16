// Starts the real engine: the Rust bridge from packages/matrix-ffi behind the typed client.

import { Engine } from './client/engine';
import type { PushContent, PushTarget } from './generated/types';
import { Bridge, CameraFailure, fetchPush, registerNativeCamera, SecretFailure } from './index';

/** The platform's secret storage, as the app implements it. Calls may block. */
export interface SecretStore {
  get(key: string): string | null;
  set(key: string, value: string): void;
  delete(key: string): void;
}

export interface NativeEngineConfig {
  /** Private data directory (a plain path, not a `file://` URI). */
  dataDir: string;
  /** Cache directory for fetched media (a plain path). */
  cacheDir: string;
  clientName: string;
  clientUri: string;
  /** The private-use redirect URI OAuth sign-ins return to. */
  oauthRedirect?: string;
}

/** Starts the engine; saved accounts restore at once and report on the returned engine. */
export function startNativeEngine(config: NativeEngineConfig, secrets: SecretStore): Engine {
  let bridge: ReturnType<typeof Bridge.start> | null = null;
  const engine = new Engine({
    send(json) {
      if (!bridge) throw new Error('the engine is not running');
      return Number(bridge.send(json));
    },
  });
  bridge = Bridge.start(config, nativeSecrets(secrets), { onEvent: (json) => engine.emitJson(json) });
  return engine;
}

/** The app's secret store as the bridge calls it, its failures as the bridge's error. */
function nativeSecrets(secrets: SecretStore) {
  const failed = (error: unknown) =>
    new SecretFailure.Failed({ reason: error instanceof Error ? error.message : String(error) });
  return {
    get(key: string) {
      try {
        return secrets.get(key) ?? undefined;
      } catch (error) {
        throw failed(error);
      }
    },
    set(key: string, value: string) {
      try {
        secrets.set(key, value);
      } catch (error) {
        throw failed(error);
      }
    },
    delete_(key: string) {
      try {
        secrets.delete(key);
      } catch (error) {
        throw failed(error);
      }
    },
  };
}

/**
 * Fetches and decrypts what a push is about without the engine (`matrix_ffi::push`): opens the
 * account's store under `dataDir`, reads the event, closes it. For a JavaScript context that
 * hasn't started the engine (Android's headless push task); with the engine running, send it
 * `FetchNotification` instead. Never rejects for the event: what can't be read is `Hidden`.
 */
export function fetchPushWithoutEngine(dataDir: string, target: PushTarget, secrets: SecretStore): Promise<PushContent> {
  return new Promise((resolve, reject) => {
    try {
      fetchPush(dataDir, JSON.stringify(target), nativeSecrets(secrets), {
        onContent: (json) => resolve(JSON.parse(json) as PushContent),
      });
    } catch (error) {
      reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
}

/** The system's camera API, as the app implements it (a native module). Calls may block. */
export interface CameraApi {
  /** The cameras, by name, the default first. */
  cameras(): string[];
  /** Starts filming with the named camera (the default when null); throws when it can't. */
  start(device: string | null): void;
  stop(): void;
}

/**
 * Lets the engine film calls with `camera` (phones have no camera backend in the engine). Its
 * frames reach the engine natively; only starting and stopping go through JavaScript.
 */
export function registerCamera(camera: CameraApi): void {
  registerNativeCamera({
    cameras: () => camera.cameras(),
    start(device) {
      try {
        camera.start(device ?? null);
      } catch (error) {
        throw new CameraFailure.Failed({ reason: error instanceof Error ? error.message : String(error) });
      }
    },
    stop: () => camera.stop(),
  });
}
