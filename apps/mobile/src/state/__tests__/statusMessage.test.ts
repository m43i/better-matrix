import { clearStatusAt, STATUS_DURATIONS, statusMessageDeadline } from '../statusMessage';

describe('status message expiry', () => {
  it('should drop a deadline that passed while the editor was open', () => {
    expect(clearStatusAt({ at: 120_000 }, 120_001)).toBeNull();
  });

  it('should count a preset from the time the status is saved', () => {
    expect(clearStatusAt(STATUS_DURATIONS[1], 120_000)).toBe(1_920_000);
  });

  it('should hide the saved deadline when the message changed on another client', () => {
    const account = {
      statusMessage: 'Away for lunch',
      statusMessageExpiry: { message: 'In a meeting', clear_at_ms: 180_000 },
    };

    expect(statusMessageDeadline(account, 120_000)).toBeNull();
  });
});
