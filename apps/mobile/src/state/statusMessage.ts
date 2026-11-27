// The account's recorded deadline and the durations offered when setting a status message.

import type { AccountData } from './types';

export const STATUS_DURATIONS = [
  { id: 'never', label: 'status_message.expiry_never' },
  { id: 'hour', label: 'schedule.hour' },
  { id: 'today', label: 'status_message.expiry_today' },
  { id: 'week', label: 'status_message.expiry_week' },
] as const;

export type StatusExpiry = (typeof STATUS_DURATIONS)[number] | { at: number };

/**
 * A preset counts from Save: an hour later, the next local midnight ("today"), or the local
 * midnight that starts next Monday ("this week", so the end of Sunday). A past deadline is
 * discarded when saving a new message.
 */
export function clearStatusAt(expiry: StatusExpiry, now: number): number | null {
  if ('at' in expiry) return expiry.at > now ? expiry.at : null;

  switch (expiry.id) {
    case 'never':
      return null;
    case 'hour':
      return now + 60 * 60_000;
    case 'today':
      return dayStart(now, 1);
    case 'week':
      // getDay: Sunday is 0. Monday is 7 days off on a Monday, 1 on a Sunday.
      return dayStart(now, 7 - ((new Date(now).getDay() + 6) % 7));
  }
}

/** The local midnight `days` days after the day of `now`. */
function dayStart(now: number, days: number): number {
  const date = new Date(now);

  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days).getTime();
}

/** Display the saved deadline only while it belongs to the status message on screen. */
export function statusMessageDeadline(
  {
    statusMessage,
    statusMessageExpiry,
  }: Pick<AccountData, 'statusMessage' | 'statusMessageExpiry'>,
  now: number,
): number | null {
  if (!statusMessageExpiry || statusMessageExpiry.message !== statusMessage) return null;

  return clearStatusAt({ at: statusMessageExpiry.clear_at_ms }, now);
}
