// The choices the settings pages list, in order, with the desktop's labels.

import type { NotificationMode } from '@better-matrix/matrix-native/types';

import { type Key, type Language, LOCALES, resolve, t } from '../../i18n';
import type { Appearance } from '../../state/settings';

/** Each language the app offers: the system's first, named with what it resolves to. */
export function languageChoices(): Array<{ id: Language; label: string }> {
  const system = LOCALES.find((l) => l.id === resolve('system'))?.label ?? 'English';
  return [{ id: 'system', label: t('settings.language_system', { language: system }) }, ...LOCALES];
}

export const APPEARANCES: ReadonlyArray<{ id: Appearance; label: Key }> = [
  { id: 'system', label: 'appearance.system' },
  { id: 'light', label: 'appearance.light' },
  { id: 'dark', label: 'appearance.dark' },
];

/** When rooms notify, as the desktop's room menu lists the modes. */
export const NOTIFICATION_MODES: ReadonlyArray<{ id: NotificationMode; label: Key; hint: Key }> = [
  { id: 'AllMessages', label: 'notify.all', hint: 'notify.all_hint' },
  { id: 'MentionsOnly', label: 'notify.mentions', hint: 'notify.mentions_hint' },
  { id: 'Mute', label: 'notify.mute', hint: 'notify.mute_hint' },
];
