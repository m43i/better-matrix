import type { ScheduledMessage } from '@better-matrix/matrix-native/types';

import { MAX_SCHEDULE_MS } from '../outgoing';
import {
  askedAnyway,
  delayUntil,
  failedIn,
  newlyFailed,
  nextRelist,
  presetTime,
  RELIST_RETRY_MS,
  relistStep,
  scheduledIn,
  sendTimeText,
  soonestScheduled,
  timerDelay,
  withTimeOf,
} from '../schedule';

/** Wednesday 18 November 2026, 14:20 local time. */
const NOW = new Date(2026, 10, 18, 14, 20).getTime();

describe('presetTime', () => {
  it('should send in 30 minutes', () => {
    expect(presetTime('in30', NOW) - NOW).toBe(30 * 60_000);
  });

  it('should send tomorrow at 9', () => {
    expect(new Date(presetTime('tomorrow', NOW))).toEqual(new Date(2026, 10, 19, 9, 0));
  });

  it('should send next Monday at 9', () => {
    expect(new Date(presetTime('monday', NOW))).toEqual(new Date(2026, 10, 23, 9, 0));
  });

  it('should send a week later on a Monday', () => {
    const monday = new Date(2026, 10, 23, 8, 0).getTime();
    expect(new Date(presetTime('monday', monday))).toEqual(new Date(2026, 10, 30, 9, 0));
  });
});

describe('delayUntil', () => {
  it('should hold a message until its time', () => {
    expect(delayUntil(NOW + 5_000, NOW)).toBe(5_000);
  });

  it('should not schedule into the past', () => {
    expect(delayUntil(NOW - 1, NOW)).toBeNull();
  });

  it('should not schedule more than 30 days ahead', () => {
    expect(delayUntil(NOW + MAX_SCHEDULE_MS + 1, NOW)).toBeNull();
  });
});

describe('scheduledIn', () => {
  it('should list one room’s scheduled messages, soonest first', () => {
    const at = (delay: string, room: string, sendAt: number) => ({
      delay_id: delay,
      room_id: room,
      send_at_ms: sendAt,
      body: null,
      failed: null,
    });

    expect(
      scheduledIn([at('b', '!r', 2), at('x', '!other', 0), at('a', '!r', 1)], '!r').map(
        (m) => m.delay_id,
      ),
    ).toEqual(['a', 'b']);
  });
});

describe('withTimeOf', () => {
  it('should take the day from one time and the clock from another', () => {
    const day = new Date(2026, 11, 1, 23, 59).getTime();
    const clock = new Date(2000, 0, 1, 7, 45, 30).getTime();
    expect(new Date(withTimeOf(day, clock))).toEqual(new Date(2026, 11, 1, 7, 45));
  });
});

describe('sendTimeText', () => {
  it('should say tomorrow for tomorrow', () => {
    expect(sendTimeText(new Date(2026, 10, 19, 9, 0).getTime(), NOW)).toBe('Tomorrow · 09:00');
  });

  it('should say today for today', () => {
    expect(sendTimeText(NOW + 60 * 60_000, NOW)).toBe('Today · 15:20');
  });
});

describe('asking again for the scheduled messages', () => {
  const at = (room_id: string, send_at_ms: number): ScheduledMessage => ({
    delay_id: `d${send_at_ms}`,
    room_id,
    send_at_ms,
    body: 'later',
    failed: null,
  });

  it('should be due when the room’s soonest message sends', () => {
    expect(soonestScheduled([at('!a', NOW + 9), at('!b', NOW + 1), at('!a', NOW + 5)], '!a')).toBe(
      NOW + 5,
    );
  });

  it('should never be due for a message the server failed to send', () => {
    const failed = { ...at('!a', NOW + 1), failed: 'No posting' };

    expect(soonestScheduled([failed, at('!a', NOW + 5)], '!a')).toBe(NOW + 5);
    expect(soonestScheduled([failed], '!a')).toBeNull();
  });

  it('should count the room’s messages the server failed to send', () => {
    expect(failedIn([{ ...at('!a', 1), failed: 'No posting' }, at('!a', 2)], '!a')).toBe(1);
  });

  it('should tell a failure once', () => {
    const failed = [{ ...at('!a', 1), failed: 'No posting' }];

    expect(newlyFailed([at('!a', 1)], failed)).toBe('No posting');
    expect(newlyFailed(failed, failed)).toBeNull();
  });

  it('should never ask for a room with nothing scheduled', () => {
    expect(nextRelist(soonestScheduled([], '!a'), null)).toBeNull();
  });

  it('should ask at the soonest message’s send time', () => {
    expect(nextRelist(NOW + 60_000, null)).toBe(NOW + 60_000);
  });

  it('should ask once when a message is due, not again at once', () => {
    const first = relistStep(NOW, null, NOW + 10);
    const second = relistStep(NOW, first.relisted, NOW + 20);
    expect([first.ask, second.ask]).toEqual([true, false]);
  });

  it('should ask again after a while when the server still lists the message as due', () => {
    const first = relistStep(NOW, null, NOW);
    expect(relistStep(NOW, first.relisted, NOW + RELIST_RETRY_MS).ask).toBe(true);
  });

  it('should ask at the next message’s time once the due one is gone', () => {
    expect(nextRelist(NOW + 90_000, { due: NOW, at: NOW })).toBe(NOW + 90_000);
  });

  it('should not ask before the message is due', () => {
    expect(relistStep(NOW + 1_000, null, NOW)).toEqual({ ask: false, relisted: null });
  });

  it('should not ask again for a due message the list was just asked for anyway', () => {
    const relisted = askedAnyway(NOW - 1_000, null, NOW);
    expect(relistStep(NOW - 1_000, relisted, NOW + 1).ask).toBe(false);
  });

  it('should still ask at a message’s time when the list was asked for before it', () => {
    expect(askedAnyway(NOW + 1_000, null, NOW)).toBeNull();
  });

  it('should wait for a time in the past not at all', () => {
    expect(timerDelay(NOW - 5, NOW)).toBe(0);
  });

  it('should wait no longer than a timer can, for a message weeks away', () => {
    expect(timerDelay(NOW + 29 * 24 * 3_600_000, NOW)).toBe(2 ** 31 - 1);
  });
});
