// The phone's side of calls (ADR 0010): libwebrtc's start-up on Android, the system camera that
// films for the engine, the audio route (earpiece or speaker), and the native view that shows a
// call's video. Frames go between the engine and the platform natively, never through
// JavaScript. Missing (Expo Go, tests, web), `CallMedia` is null and `CallVideoView` too, and
// the stage shows faces instead of video.

import { requireNativeView, requireOptionalNativeModule } from 'expo';
import type { ComponentType } from 'react';
import type { ViewProps } from 'react-native';

export interface CallMediaModule {
  /** Gets libwebrtc ready to use the microphone and speaker (Android's application context). */
  prepare(): boolean;
  /** The cameras, by name ("front", "back", …), the front one first. */
  cameras(): string[];
  /** Starts filming with the named camera (else the front one); throws when it can't. */
  startCamera(device: string | null): void;
  stopCamera(): void;
  /** Puts the phone in (or out of) call audio: echo cancellation, the call's volume. */
  setAudioActive(active: boolean): void;
  /** Plays the call on the loudspeaker rather than the earpiece. */
  setSpeaker(on: boolean): void;
}

export const CallMedia = requireOptionalNativeModule<CallMediaModule>('CallMedia');

export interface CallVideoProps extends ViewProps {
  /** The engine's `VideoId` of the video to show. */
  video: number;
  /** Show the whole frame with bars (a shared screen) rather than filling the view. */
  contain?: boolean;
  /** Mirror it, as the user's own camera is shown. */
  mirror?: boolean;
}

/** Shows one video of the call under way, drawn natively from the engine's frames. */
export const CallVideoView: ComponentType<CallVideoProps> | null = CallMedia
  ? requireNativeView<CallVideoProps>('CallMedia')
  : null;
