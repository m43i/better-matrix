import { FIRST_WAIT_MS, LONGEST_WAIT_MS, Retries, retryWait } from '../retry';

const NOW = 1_000_000;

describe('Retries', () => {
  it('a failed fetch should wait before it is tried again', () => {
    const retries = new Retries<string>();
    retries.failed('a', NOW);
    expect(retries.mayTry('a', NOW)).toBe(false);
    expect(retries.mayTry('a', NOW + FIRST_WAIT_MS)).toBe(true);
  });

  it('each failure in a row should wait longer', () => {
    const retries = new Retries<string>();
    retries.failed('a', NOW);
    retries.failed('a', NOW);
    expect(retries.mayTry('a', NOW + FIRST_WAIT_MS)).toBe(false);
    expect(retries.mayTry('a', NOW + FIRST_WAIT_MS * 2)).toBe(true);
  });

  it('waits should stop growing at five minutes', () => {
    expect(retryWait(40)).toBe(LONGEST_WAIT_MS);
  });

  it('a success should forget the failures', () => {
    const retries = new Retries<string>();
    retries.failed('a', NOW);
    retries.succeeded('a');
    expect(retries.mayTry('a', NOW) && !retries.hasFailed('a')).toBe(true);
  });

  it('past the limit the failures tried again soonest should be forgotten', () => {
    const retries = new Retries<string>();
    retries.failed('old', NOW);
    retries.failed('new', NOW + FIRST_WAIT_MS);
    retries.keepLatest(1);
    expect([retries.hasFailed('old'), retries.hasFailed('new')]).toEqual([false, true]);
  });
});
