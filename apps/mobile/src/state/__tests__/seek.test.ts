import {
  openSeek,
  quoteHere,
  quoteTapped,
  type SeekProgress,
  seekEnded,
  seekStep,
  sought,
} from '../seek';

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

describe('quoteHere', () => {
  it('a quote the engine places here should be looked for again, past the page budget', () => {
    const gaveUp = seekEnded(quoteTapped(openSeek(null), '$b'), '$b');
    const seek = quoteHere(gaveUp, '$b');
    expect([sought(seek), seek.quoteHere]).toEqual(['$b', true]);
  });

  it('a quote placed here should not take over another quote’s search', () => {
    const other = quoteTapped(openSeek(null), '$c');
    expect(quoteHere(other, '$b')).toBe(other);
  });

  it('a quote tapped since should look within the budget again', () => {
    const here = quoteHere(openSeek(null), '$b');
    expect(quoteTapped(here, '$c').quoteHere).toBe(false);
  });

  it('a quote placed here and found should end its search', () => {
    const found = seekEnded(quoteHere(openSeek(null), '$b'), '$b');
    expect([sought(found), found.quoteHere]).toEqual([null, false]);
  });
});

describe('seekStep', () => {
  const progress: SeekProgress = {
    quote: true,
    wanted: true,
    loaded: false,
    busy: false,
    atStart: false,
    pages: 0,
    budget: 10,
    unbounded: false,
  };

  it('a quote whose jump was superseded should stop', () => {
    expect(seekStep({ ...progress, wanted: false, loaded: true })).toBe('stop');
  });

  it('a loaded message should be scrolled to', () => {
    expect(seekStep({ ...progress, loaded: true })).toBe('scroll');
  });

  it('a search should wait while a page is coming', () => {
    expect(seekStep({ ...progress, busy: true })).toBe('wait');
  });

  it('a search within its budget should page back', () => {
    expect(seekStep({ ...progress, pages: 9 })).toBe('page');
  });

  it('a quote out of pages should be asked about elsewhere', () => {
    expect(seekStep({ ...progress, pages: 10 })).toBe('elsewhere');
  });

  it('a quote the engine places here should page on past its budget', () => {
    expect(seekStep({ ...progress, pages: 50, unbounded: true })).toBe('page');
  });

  it('a quote the engine places here should be missing once history starts', () => {
    expect(seekStep({ ...progress, atStart: true, unbounded: true })).toBe('missing');
  });

  it('a local hit should page on until history starts, then end', () => {
    const hit = { ...progress, quote: false, unbounded: true };
    expect([seekStep({ ...hit, pages: 50 }), seekStep({ ...hit, atStart: true })]).toEqual([
      'page',
      'elsewhere',
    ]);
  });

  it('a link’s target should go on looking when its search has no jump to supersede', () => {
    expect(seekStep({ ...progress, quote: false, wanted: false })).toBe('page');
  });
});
