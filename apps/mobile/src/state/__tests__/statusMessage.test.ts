import { clearStatusAt, STATUS_DURATIONS, statusMessageDeadline } from '../statusMessage';

describe('status message expiry', () => {
  it('should drop a deadline that passed while the editor was open', () => {
    expect(clearStatusAt({ at: 120_000 }, 120_001)).toBeNull();
  });

  it('should clear an hour after the status is saved', () => {
    expect(clearStatusAt(STATUS_DURATIONS[1], 120_000)).toBe(3_720_000);
  });

  it('should clear today at the next local midnight', () => {
    const friday = new Date(2026, 10, 27, 14, 0).getTime();

    expect(clearStatusAt(STATUS_DURATIONS[2], friday)).toBe(new Date(2026, 10, 28).getTime());
  });

  it('should clear this week when Sunday ends', () => {
    const friday = new Date(2026, 10, 27, 14, 0).getTime();

    expect(clearStatusAt(STATUS_DURATIONS[3], friday)).toBe(new Date(2026, 10, 30).getTime());
  });

  it('should clear this week at midnight when set on a Sunday', () => {
    const sunday = new Date(2026, 10, 29, 23, 0).getTime();

    expect(clearStatusAt(STATUS_DURATIONS[3], sunday)).toBe(new Date(2026, 10, 30).getTime());
  });

  it('should keep this week for seven days when set on a Monday', () => {
    const monday = new Date(2026, 10, 30, 0, 0).getTime();

    expect(clearStatusAt(STATUS_DURATIONS[3], monday)).toBe(new Date(2026, 11, 7).getTime());
  });

  it('should hide the saved deadline when the message changed on another client', () => {
    const account = {
      statusMessage: 'Away for lunch',
      statusMessageExpiry: { message: 'In a meeting', clear_at_ms: 180_000 },
    };

    expect(statusMessageDeadline(account, 120_000)).toBeNull();
  });
});
