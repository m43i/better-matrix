// The words people and rooms are shown with, in the desktop's terms.

import type {
  HistoryVisibility,
  MemberRole,
  NotificationMode,
} from '@better-matrix/matrix-native/types';

import { type Key, t } from '../../i18n';
import { dayOf } from '../../state/people';

/** A role's name; `Member` too (the desktop leaves it off member rows, menus name it). */
export function roleLabel(role: MemberRole): string {
  switch (role) {
    case 'Admin':
      return t('members.admin');
    case 'Moderator':
      return t('members.moderator');
    case 'Member':
      return t('members.member');
  }
}

export function notificationLabel(mode: NotificationMode): string {
  switch (mode) {
    case 'AllMessages':
      return t('notify.all');
    case 'MentionsOnly':
      return t('notify.mentions');
    case 'Mute':
      return t('notify.mute');
  }
}

export function notificationHint(mode: NotificationMode): string {
  switch (mode) {
    case 'AllMessages':
      return t('notify.all_hint');
    case 'MentionsOnly':
      return t('notify.mentions_hint');
    case 'Mute':
      return t('notify.mute_hint');
  }
}

/** Who may read a room's earlier messages, as a sentence. */
export function historyLabel(history: HistoryVisibility): string {
  switch (history) {
    case 'WorldReadable':
      return t('mobile.people.history.world_readable');
    case 'Shared':
      return t('mobile.people.history.shared');
    case 'Invited':
      return t('mobile.people.history.invited');
    case 'Joined':
      return t('mobile.people.history.joined');
  }
}

/** When something happened, as the desktop's lists show it: the day, then the time. */
export function when(ms: number, now: number = Date.now()): string {
  const day = dayOf(ms, now);

  // SAFETY: dayOf returns calendar months 1–12; every catalog defines the twelve date.month_XX keys.
  const date =
    day === 'today'
      ? t('date.today')
      : day === 'yesterday'
        ? t('date.yesterday')
        : t('date.format', {
            month: t(`date.month_${String(day.month).padStart(2, '0')}` as Key),
            day: String(day.day),
            year: String(day.year),
          });

  const at = new Date(ms);
  const time = `${String(at.getHours()).padStart(2, '0')}:${String(at.getMinutes()).padStart(2, '0')}`;

  return `${date}, ${time}`;
}
