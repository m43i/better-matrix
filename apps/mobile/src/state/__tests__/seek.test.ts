import { openSeek, quoteTapped, seekEnded, sought } from '../seek';

describe('openSeek', () => {
  it('a timeline opened at the newest should be positioned at once', () => {
    expect([openSeek(null).positioned, sought(openSeek(null))]).toEqual([true, null]);
  });

  it('a timeline opened at a link’s message should look for it, not yet positioned', () => {
    expect([openSeek('$a').positioned, sought(openSeek('$a'))]).toEqual([false, '$a']);
  });
});

describe('seekEnded', () => {
  it('a timeline that got to its target should be positioned and look for nothing more', () => {
    const seek = seekEnded(openSeek('$a'), '$a');
    expect([seek.positioned, sought(seek)]).toEqual([true, null]);
  });

  it('another message’s search ending should leave the opening waiting for its target', () => {
    expect(seekEnded(openSeek('$a'), '$b').positioned).toBe(false);
  });

  it('a quote found after the opening should keep the timeline positioned', () => {
    const atTarget = seekEnded(openSeek('$a'), '$a');
    const found = seekEnded(quoteTapped(atTarget, '$b'), '$b');
    expect([found.positioned, sought(found)]).toEqual([true, null]);
  });

  it('an earlier quote’s search ending should leave a newer quote’s going', () => {
    const newer = quoteTapped(quoteTapped(openSeek(null), '$b'), '$c');
    expect(sought(seekEnded(newer, '$b'))).toBe('$c');
  });
});

describe('quoteTapped', () => {
  it('a quote not loaded should be looked for while the timeline stays positioned', () => {
    const seek = quoteTapped(seekEnded(openSeek('$a'), '$a'), '$b');
    expect([seek.positioned, sought(seek)]).toEqual([true, '$b']);
  });

  it('a quote tapped before the target was found should end the opening', () => {
    const seek = quoteTapped(openSeek('$a'), '$b');
    expect([seek.positioned, sought(seek)]).toEqual([true, '$b']);
  });

  it('a quote on hand should stop the search for the target', () => {
    const seek = quoteTapped(openSeek('$a'), null);
    expect([seek.positioned, sought(seek)]).toEqual([true, null]);
  });

  it('a quote on hand should stop an earlier quote’s search', () => {
    expect(sought(quoteTapped(quoteTapped(openSeek(null), '$b'), null))).toBeNull();
  });

  it('the target found after a quote was tapped should not take the search back', () => {
    const seek = seekEnded(quoteTapped(openSeek('$a'), '$b'), '$a');
    expect(sought(seek)).toBe('$b');
  });
});
