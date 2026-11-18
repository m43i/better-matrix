import { minuteClock } from '../useNow';

const MINUTE = 60_000;

/** Half past some minute. */
const START = 1_800_000_000_000 + MINUTE / 2;

beforeEach(() => {
  jest.useFakeTimers();
  jest.setSystemTime(START);
});

afterEach(() => jest.useRealTimers());

describe('minuteClock', () => {
  it('should tick at the next minute boundary', () => {
    const clock = minuteClock();
    const listener = jest.fn();
    clock.subscribe(listener);
    jest.advanceTimersByTime(MINUTE / 2);

    expect([listener.mock.calls.length, clock.snapshot()]).toEqual([1, START + MINUTE / 2]);
  });

  it('should not tick within the minute', () => {
    const clock = minuteClock();
    const listener = jest.fn();
    clock.subscribe(listener);
    jest.advanceTimersByTime(MINUTE / 2 - 1);

    expect(listener).not.toHaveBeenCalled();
  });

  it('should stop once nobody listens', () => {
    const clock = minuteClock();
    clock.subscribe(() => undefined)();

    expect(jest.getTimerCount()).toBe(0);
  });

  it('should show the current minute when listened to again after a while', () => {
    const clock = minuteClock();
    clock.subscribe(() => undefined)();
    jest.setSystemTime(START + 5 * MINUTE);
    clock.subscribe(() => undefined);

    expect(clock.snapshot()).toBe(START + 5 * MINUTE);
  });
});
