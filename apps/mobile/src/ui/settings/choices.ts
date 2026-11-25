// The choices the settings pages list, in order, with the desktop's labels.

import type { NotificationMode } from '@better-matrix/matrix-native/types';

import { type Key, type Language, LOCALES, resolve, t } from '../../i18n';
import type { Appearance, MessageLayout } from '../../state/settings';

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

/** How messages lay out, as the desktop's Appearance settings list them. */
export const MESSAGE_LAYOUTS: ReadonlyArray<{ id: MessageLayout; label: Key; hint: Key }> = [
  { id: 'rows', label: 'message_layout.rows', hint: 'message_layout.rows_hint' },
  { id: 'bubbles', label: 'message_layout.bubbles', hint: 'message_layout.bubbles_hint' },
];

/** Where room changes show, as the desktop's General settings list the choices. */
export const ROOM_CHANGES: ReadonlyArray<{ on: boolean; label: Key }> = [
  { on: true, label: 'settings.room_changes_on' },
  { on: false, label: 'settings.room_changes_off' },
];

/** When rooms notify, as the desktop's room menu lists the modes. */
export const NOTIFICATION_MODES: ReadonlyArray<{ id: NotificationMode; label: Key; hint: Key }> = [
  { id: 'AllMessages', label: 'notify.all', hint: 'notify.all_hint' },
  { id: 'MentionsOnly', label: 'notify.mentions', hint: 'notify.mentions_hint' },
  { id: 'Mute', label: 'notify.mute', hint: 'notify.mute_hint' },
];
