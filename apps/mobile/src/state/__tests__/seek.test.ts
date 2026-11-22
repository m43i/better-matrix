import {
  openSeek,
  quoteHere,
  quoteTapped,
  reachParam,
  type SeekProgress,
  seekEnded,
  seekStep,
  sought,
  soughtReach,
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
    reach: 'near',
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
    expect(seekStep({ ...progress, pages: 50, reach: 'located' })).toBe('page');
  });

  it('a quote the engine places here should be missing once history starts', () => {
    expect(seekStep({ ...progress, atStart: true, reach: 'located' })).toBe('missing');
  });

  it('a local hit should page on until history starts, then end', () => {
    const hit: SeekProgress = { ...progress, quote: false, reach: 'local' };
    expect([seekStep({ ...hit, pages: 50 }), seekStep({ ...hit, atStart: true })]).toEqual([
      'page',
      'elsewhere',
    ]);
  });

  it('a target the engine located here should page on past its budget', () => {
    const located: SeekProgress = { ...progress, quote: false, reach: 'located' };
    expect(seekStep({ ...located, pages: 50 })).toBe('page');
  });

  it('a target the engine located here should be missing once history starts', () => {
    const located: SeekProgress = { ...progress, quote: false, reach: 'located' };
    expect(seekStep({ ...located, pages: 50, atStart: true })).toBe('missing');
  });

  it('a located target whose jump was superseded should stop', () => {
    const located: SeekProgress = { ...progress, quote: false, reach: 'located' };
    expect(seekStep({ ...located, wanted: false })).toBe('stop');
  });

  it('a link’s target out of pages should end', () => {
    expect(seekStep({ ...progress, quote: false, pages: 10 })).toBe('elsewhere');
  });
});

describe('soughtReach', () => {
  it('a link’s target should be looked for as far as its route says', () => {
    expect(soughtReach(openSeek('$a', 'located'))).toBe('located');
  });

  it('a target opened without a reach should be looked for a few pages back', () => {
    expect(soughtReach(openSeek('$a'))).toBe('near');
  });

  it('a quote tapped should be looked for a few pages back, whatever the target’s reach', () => {
    expect(soughtReach(quoteTapped(openSeek('$a', 'located'), '$b'))).toBe('near');
  });

  it('a quote the engine places here should be looked for to the start of history', () => {
    expect(soughtReach(quoteHere(openSeek(null), '$b'))).toBe('located');
  });
});

describe('reachParam', () => {
  it('a route’s reach should read back as the reach it was written as', () => {
    expect([reachParam('local'), reachParam('located')]).toEqual(['local', 'located']);
  });

  it('a missing or unknown reach should be near', () => {
    expect([reachParam(undefined), reachParam('far')]).toEqual(['near', 'near']);
  });
});
