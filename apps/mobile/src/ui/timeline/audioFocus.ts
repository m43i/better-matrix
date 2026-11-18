// One clip plays at a time, as on the desktop: a card that starts playing pauses the one before.

import type { AudioPlayer } from 'expo-audio';

let playing: AudioPlayer | null = null;

/** `player` is about to play: the one playing before pauses. */
export function claimAudio(player: AudioPlayer): void {
  if (playing && playing !== player) {
    // A card that went away has released its player, which then can't pause.
    try {
      playing.pause();
    } catch {
      // Nothing to pause.
    }
  }

  playing = player;
}

/** `player` is going away. */
export function releaseAudio(player: AudioPlayer): void {
  if (playing === player) playing = null;
}
