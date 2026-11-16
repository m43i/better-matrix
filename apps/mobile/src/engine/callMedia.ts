// The phone's side of calls: permissions, libwebrtc's start-up, the camera the engine films with,
// and the audio route. The native half is the local module `modules/call-media`; without it
// (Expo Go, tests) calls still go through, as voice, and the stage shows faces.

import { useSyncExternalStore } from 'react';

import { CallMedia } from '../../modules/call-media';

/** What the user allowed for a call. */
export interface CallPermissions {
  microphone: boolean;
  camera: boolean;
}

/** What the user allowed so far this run (Android restarts the app when one is taken back). */
const allowed: CallPermissions = { microphone: false, camera: false };

/**
 * The permissions a call needs, when no question has to be asked: all granted already, or no
 * native call media to ask for (the engine then tries on its own). `null` means ask first.
 */
export function knownPermissions(video: boolean): CallPermissions | null {
  if (!CallMedia) return { microphone: true, camera: video };

  if (allowed.microphone && (!video || allowed.camera)) return { microphone: true, camera: video };

  return null;
}

/** Asks for the microphone, and the camera when `video`; the system asks only the first time. */
export async function askCallPermissions(video: boolean): Promise<CallPermissions> {
  // Loaded here: neither module loads without its native half (tests).
  // SAFETY: this literal require resolves the installed expo-audio module; it is lazy for native-free tests.
  const { requestRecordingPermissionsAsync } = require('expo-audio') as typeof import('expo-audio');

  // SAFETY: this literal require resolves the installed expo-image-picker module.
  const { requestCameraPermissionsAsync } =
    require('expo-image-picker') as typeof import('expo-image-picker');

  allowed.microphone = (await requestRecordingPermissionsAsync()).granted;

  if (video) allowed.camera = (await requestCameraPermissionsAsync()).granted;

  return { microphone: allowed.microphone, camera: video && allowed.camera };
}

let prepared = false;

/** Gets the native side ready before the first call: libwebrtc's context, the engine's camera. */
export function prepareCallMedia(): void {
  const media = CallMedia;

  if (prepared || !media) return;
  prepared = true;
  media.prepare();

  // The engine's native half, loaded only where the native module is (not in tests).
  // SAFETY: this literal require resolves our typed native entry point, after the native module was found.
  const { registerCamera } =
    require('@better-matrix/matrix-native/native') as typeof import('@better-matrix/matrix-native/native');

  registerCamera({
    cameras: () => media.cameras(),
    start: (device) => media.startCamera(device),
    stop: () => media.stopCamera(),
  });
}

/** Whether the app can film and show video (the native module is there). */
export function canShowVideo(): boolean {
  return CallMedia !== null;
}

// ----- the audio route -----------------------------------------------------------------------

let speaker = false;

const listeners = new Set<() => void>();

/** Starts (or ends) the phone's call audio; a video call starts on the loudspeaker. */
export function setCallAudio(active: boolean, video: boolean): void {
  CallMedia?.setAudioActive(active);

  if (active) setSpeaker(video);
}

/** Plays the call on the loudspeaker (`true`) or the earpiece. */
export function setSpeaker(on: boolean): void {
  speaker = on;
  CallMedia?.setSpeaker(on);

  for (const listener of listeners) listener();
}

/** Whether the call plays on the loudspeaker. */
export function useSpeaker(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);

      return () => listeners.delete(listener);
    },
    () => speaker,
  );
}

/** Whether the phone can choose between earpiece and loudspeaker. */
export function canRouteAudio(): boolean {
  return CallMedia !== null;
}

/** The cameras the phone has, for switching. */
export function cameraNames(): string[] {
  return CallMedia?.cameras() ?? [];
}
