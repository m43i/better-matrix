// Scheduled messages (MSC4140 delayed events): when the schedule sheet's choices send, as
// Slack's do (in 30 minutes, in an hour, tomorrow at 9, next Monday at 9, or a time picked), and
// which of the account's scheduled messages belong to a conversation. Times are the phone's
// local time; the server holds the message for the delay the app works out from them.

import type { ScheduledMessage } from '@better-matrix/matrix-native/types';

import { t } from '../i18n';
import { MAX_SCHEDULE_MS } from './outgoing';
import { clockTime, dateLabel } from './timelineText';

export const PRESETS = ['in30', 'inHour', 'tomorrow', 'monday'] as const;

export type SchedulePreset = (typeof PRESETS)[number];

/** The hour the morning choices send at. */
const MORNING = 9;

/** When a choice sends, in ms since the epoch, from `now`. */
export function presetTime(preset: SchedulePreset, now: number): number {
  switch (preset) {
    case 'in30':
      return now + 30 * 60_000;
    case 'inHour':
      return now + 60 * 60_000;
    case 'tomorrow': {
      const at = new Date(now);
      at.setDate(at.getDate() + 1);
      at.setHours(MORNING, 0, 0, 0);

      return at.getTime();
    }

    case 'monday': {
      const at = new Date(now);
      // Monday is 1; on a Monday, the next one.
      const days = (8 - at.getDay()) % 7 || 7;
      at.setDate(at.getDate() + days);
      at.setHours(MORNING, 0, 0, 0);

      return at.getTime();
    }
  }
}

/**
 * How long the server holds a message to send it at `at`; `null` when `at` isn't a time it can
 * be scheduled for (past, or more than 30 days ahead).
 */
export function delayUntil(at: number, now: number): number | null {
  const delay = at - now;

  return delay > 0 && delay <= MAX_SCHEDULE_MS ? delay : null;
}

/** The scheduled messages of one room, soonest first. */
export function scheduledIn(
  scheduled: readonly ScheduledMessage[] | null | undefined,
  roomId: string,
): ScheduledMessage[] {
  return (scheduled ?? [])
    .filter((message) => message.room_id === roomId)
    .sort((a, b) => a.send_at_ms - b.send_at_ms);
}

/** When a room's soonest scheduled message sends; `null` when it has none. */
export function soonestScheduled(
  scheduled: readonly ScheduledMessage[] | null | undefined,
  roomId: string,
): number | null {
  return scheduledIn(scheduled, roomId)[0]?.send_at_ms ?? null;
}

/** How long to wait before asking again while the server still lists a message as due. */
export const RELIST_RETRY_MS = 30_000;

/** The last time the list was asked for because a message was due: which send time, and when. */
export interface Relisted {
  due: number;
  at: number;
}

/**
 * When to ask the server for the scheduled messages again, so a conversation left open stops
 * showing one it has sent: at the soonest one's send time, once; then, while the server still
 * lists that one, every `RELIST_RETRY_MS`. `null` when nothing is scheduled.
 */
export function nextRelist(due: number | null, relisted: Relisted | null): number | null {
  if (due === null) return null;

  return relisted?.due === due ? relisted.at + RELIST_RETRY_MS : due;
}

/** What a wake does: whether it asks, and what to remember after. */
export interface RelistStep {
  ask: boolean;
  relisted: Relisted | null;
}

/**
 * Whether to ask now (a timer can wake early: see `timerDelay`), and what to remember of it if
 * so.
 */
export function relistStep(due: number | null, relisted: Relisted | null, now: number): RelistStep {
  const next = nextRelist(due, relisted);

  if (next === null || due === null || now < next) return { ask: false, relisted };

  return { ask: true, relisted: { due, at: now } };
}

/**
 * What to remember after asking for the list for another reason (the conversation opened, the
 * app came back): that answers for a message already due, so its own wake doesn't ask again.
 */
export function askedAnyway(
  due: number | null,
  relisted: Relisted | null,
  now: number,
): Relisted | null {
  return due !== null && due <= now ? { due, at: now } : relisted;
}

/** The longest a JavaScript timer waits: a longer delay fires at once. */
const LONGEST_TIMER = 2 ** 31 - 1;

/** How long a timer waits for `next`: never less than nothing, nor past what a timer can hold. */
export function timerDelay(next: number, now: number): number {
  return Math.min(Math.max(0, next - now), LONGEST_TIMER);
}

/** `date` with its time of day from `time` (a time picker's value), seconds dropped. */
export function withTimeOf(date: number, time: number): number {
  const at = new Date(date);
  const clock = new Date(time);
  at.setHours(clock.getHours(), clock.getMinutes(), 0, 0);

  return at.getTime();
}

/** When a scheduled message sends, as people read it: "Today · 14:30", "Tomorrow · 09:00". */
export function sendTimeText(at: number, now: number): string {
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  const day = new Date(at);

  const isTomorrow =
    day.getFullYear() === tomorrow.getFullYear() &&
    day.getMonth() === tomorrow.getMonth() &&
    day.getDate() === tomorrow.getDate();

  return `${isTomorrow ? t('mobile.composer.date_tomorrow') : dateLabel(at, now)} · ${clockTime(at)}`;
}
