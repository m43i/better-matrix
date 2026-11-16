// This device's preferences, as the desktop keeps them (apps/desktop/src/state/settings.rs),
// minus what phones decide themselves (interface size, call devices).

import type { Language } from '../i18n';

export type Appearance = 'system' | 'light' | 'dark';

export interface Settings {
  language: Language;
  appearance: Appearance;
  /** Play GIFs behind Giphy, Tenor and Imgur links inline. */
  showLinkGifs: boolean;
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
}

export const DEFAULT_SETTINGS: Settings = {
  language: 'system',
  appearance: 'system',
  showLinkGifs: true,
  showLinkPreviews: true,
  ringSound: true,
  lastAccount: null,
  pushEnabled: true,
  pushOff: [],
};

/** Settings from saved JSON: unknown or broken fields fall back to their defaults. */
export function parseSettings(json: string | null): Settings {
  if (!json) return DEFAULT_SETTINGS;
  try {
    const saved = JSON.parse(json) as Partial<Settings>;
    const pick = <K extends keyof Settings>(key: K, valid: (v: unknown) => boolean): Settings[K] =>
      valid(saved[key]) ? (saved[key] as Settings[K]) : DEFAULT_SETTINGS[key];
    const isBool = (v: unknown) => typeof v === 'boolean';
    return {
      language: pick('language', (v) => ['system', 'en', 'de', 'fr'].includes(v as string)),
      appearance: pick('appearance', (v) => ['system', 'light', 'dark'].includes(v as string)),
      showLinkGifs: pick('showLinkGifs', isBool),
      showLinkPreviews: pick('showLinkPreviews', isBool),
      ringSound: pick('ringSound', isBool),
      lastAccount: pick('lastAccount', (v) => v === null || typeof v === 'string'),
      pushEnabled: pick('pushEnabled', isBool),
      pushOff: pick('pushOff', (v) => Array.isArray(v) && v.every((id) => typeof id === 'string')),
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}
