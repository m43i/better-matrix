import { jumps } from '../links/jumps';

describe('jumps', () => {
  it('a jump should be wanted while it is the latest and its screen is on show', () => {
    const wanted = jumps().begin(() => true);
    expect(wanted()).toBe(true);
  });

  it('a jump should not be wanted once a later one began', () => {
    const all = jumps();
    const first = all.begin();
    const second = all.begin();
    expect([first(), second()]).toEqual([false, true]);
  });

  it('a jump should not be wanted once its screen is no longer on show', () => {
    let shown = true;
    const wanted = jumps().begin(() => shown);
    shown = false;
    expect(wanted()).toBe(false);
  });
});
