// The account's recorded deadline and the durations offered when setting a status message.

import type { AccountData } from './types';

export const STATUS_DURATIONS = [
  { id: 'never', label: 'status_message.expiry_never', minutes: null },
  { id: 'half-hour', label: 'schedule.half_hour', minutes: 30 },
  { id: 'hour', label: 'schedule.hour', minutes: 60 },
  { id: 'four-hours', label: 'status_message.expiry_four_hours', minutes: 240 },
  { id: 'day', label: 'status_message.expiry_day', minutes: 1440 },
] as const;

export type StatusExpiry = (typeof STATUS_DURATIONS)[number] | { at: number };

/** A preset counts from Save. A past deadline is discarded when saving a new message. */
export function clearStatusAt(expiry: StatusExpiry, now: number): number | null {
  if ('at' in expiry) return expiry.at > now ? expiry.at : null;

  return expiry.minutes === null ? null : now + expiry.minutes * 60_000;
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
