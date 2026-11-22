import { MAX_SCHEDULE_MS } from '../outgoing';
import { delayUntil, presetTime, scheduledIn, sendTimeText, withTimeOf } from '../schedule';

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
