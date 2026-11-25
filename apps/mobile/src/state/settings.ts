// This device's preferences, as the desktop keeps them (apps/desktop/src/state/settings.rs),
// minus what phones decide themselves (interface size, call devices).

import { z } from 'zod';

import type { PackImage } from '@better-matrix/matrix-native/types';

import type { Language } from '../i18n';
import type { PlaybackSpeed } from './voiceCard';

export type Appearance = 'system' | 'light' | 'dark';

/**
 * How conversations lay out their messages: classic rows under each sender's name, or chat
 * bubbles, other people's at the left by their picture and the user's own at the right.
 */
export type MessageLayout = 'rows' | 'bubbles';

export interface Settings {
  language: Language;
  appearance: Appearance;
  messageLayout: MessageLayout;
  /** Play GIFs behind Giphy, Tenor and Imgur links inline. */
  showLinkGifs: boolean;
  /**
   * Draw shared places on a map from OpenStreetMap, which the device fetches (so OpenStreetMap
   * sees its address and the area shown); off, places show without one and nothing is fetched.
   */
  showMaps: boolean;
  /** Show a card for the first link in a message. */
  showLinkPreviews: boolean;
  /** Ring aloud for incoming calls. */
  ringSound: boolean;
  /** The account last on screen, to come back to. */
  lastAccount: string | null;
  /** Whether this device gets pushes while the app isn't running ("Notifications on this device"). */
  pushEnabled: boolean;
  /** Accounts whose pushes this device doesn't get, though push is on. */
  pushOff: string[];
  /** How fast voice memos and audio clips play, at their own pitch. */
  playbackSpeed: PlaybackSpeed;
  /**
   * Conversations draw the room changes only the room log lists otherwise (`logOnly`): profile
   * and room setup changes.
   */
  showRoomChanges: boolean;
  /** Timelines draw the events they otherwise leave out (developer tools). */
  showHiddenEvents: boolean;
  /** The stickers each account sent lately, newest first: the sticker picker shows them first. */
  recentStickers: Record<string, PackImage[]>;
}

export const DEFAULT_SETTINGS: Settings = {
  language: 'system',
  appearance: 'system',
  messageLayout: 'rows',
  showLinkGifs: true,
  showMaps: true,
  showLinkPreviews: true,
  ringSound: true,
  lastAccount: null,
  pushEnabled: true,
  pushOff: [],
  playbackSpeed: 1,
  showRoomChanges: false,
  showHiddenEvents: false,
  recentStickers: {},
};

const packImage = z.object({
  shortcode: z.string(),
  body: z.string(),
  url: z.string(),
  media: z.string(),
  emoticon: z.boolean(),
  sticker: z.boolean(),
  dimensions: z.tuple([z.number(), z.number()]).nullable(),
  mimetype: z.string().nullable(),
});

const savedSettings = z.object({
  language: z.enum(['system', 'en', 'de', 'fr']).catch(DEFAULT_SETTINGS.language),
  appearance: z.enum(['system', 'light', 'dark']).catch(DEFAULT_SETTINGS.appearance),
  messageLayout: z.enum(['rows', 'bubbles']).catch(DEFAULT_SETTINGS.messageLayout),
  showLinkGifs: z.boolean().catch(DEFAULT_SETTINGS.showLinkGifs),
  showMaps: z.boolean().catch(DEFAULT_SETTINGS.showMaps),
  showLinkPreviews: z.boolean().catch(DEFAULT_SETTINGS.showLinkPreviews),
  ringSound: z.boolean().catch(DEFAULT_SETTINGS.ringSound),
  lastAccount: z.string().nullable().catch(DEFAULT_SETTINGS.lastAccount),
  pushEnabled: z.boolean().catch(DEFAULT_SETTINGS.pushEnabled),
  pushOff: z.array(z.string()).catch(() => []),
  playbackSpeed: z
    .union([z.literal(1), z.literal(1.5), z.literal(2)])
    .catch(DEFAULT_SETTINGS.playbackSpeed),
  showRoomChanges: z.boolean().catch(DEFAULT_SETTINGS.showRoomChanges),
  showHiddenEvents: z.boolean().catch(DEFAULT_SETTINGS.showHiddenEvents),
  recentStickers: z.record(z.string(), z.array(packImage)).catch(() => ({})),
});

/** Settings from saved JSON: unknown or broken fields fall back to their defaults. */
export function parseSettings(json: string | null): Settings {
  if (!json) return DEFAULT_SETTINGS;

  try {
    const saved = savedSettings.safeParse(JSON.parse(json));

    return saved.success ? saved.data : DEFAULT_SETTINGS;
  } catch {
    return DEFAULT_SETTINGS;
  }
}
