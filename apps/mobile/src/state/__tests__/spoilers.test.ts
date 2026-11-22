import { type Inline } from '../html';
import {
  hiddenReading,
  inlineSpoilerId,
  NONE_REVEALED,
  revealSpoiler,
  spoilerCover,
  spoilerId,
} from '../spoilers';

describe('revealSpoiler', () => {
  it('a tapped spoiler should be shown', () => {
    const id = spoilerId('$a', '0/1', 'the butler');
    expect(revealSpoiler(NONE_REVEALED, id).has(id)).toBe(true);
  });

  it('another message’s spoiler at the same place should stay hidden', () => {
    const revealed = revealSpoiler(NONE_REVEALED, spoilerId('$a', '0/1', 'the butler'));
    expect(revealed.has(spoilerId('$b', '0/1', 'the butler'))).toBe(false);
  });

  it('an edit putting other words where a shown spoiler stood should show them hidden', () => {
    const revealed = revealSpoiler(NONE_REVEALED, spoilerId('$a', '0/1', 'the butler'));
    expect(revealed.has(spoilerId('$a', '0/1', 'the gardener'))).toBe(false);
  });

  it('another spoiler with the same words in the same message should stay hidden', () => {
    const revealed = revealSpoiler(NONE_REVEALED, spoilerId('$a', '0/1', 'yes'));
    expect(revealed.has(spoilerId('$a', '0/3', 'yes'))).toBe(false);
  });

  it('a shown spoiler drawn again should stay shown', () => {
    const revealed = revealSpoiler(NONE_REVEALED, spoilerId('$a', '0/1', 'the butler'));
    expect(revealed.has(spoilerId('$a', '0/1', 'the butler'))).toBe(true);
  });

  it('tapping a shown spoiler again should leave the set as it was', () => {
    const id = spoilerId('$a', '0/1', 'the butler');
    const revealed = revealSpoiler(NONE_REVEALED, id);
    expect(revealSpoiler(revealed, id)).toBe(revealed);
  });
});

describe('spoilerCover', () => {
  it('should hold none of the spoiler’s words', () =>
    expect(spoilerCover('the butler did it')).not.toMatch(/[a-z]/));

  it('should draw a blank for each character', () =>
    expect([...spoilerCover('he dies')]).toHaveLength(7));

  it('should keep the spaces between words, so it wraps like them', () =>
    expect(
      spoilerCover('he dies')
        .split(' ')
        .map((w) => w.length),
    ).toEqual([2, 4]));

  it('should keep line breaks', () => expect(spoilerCover('a\nb').split('\n')).toHaveLength(2));

  it('should cover an emoji as one blank', () => expect([...spoilerCover('🔪')]).toHaveLength(1));
});

describe('hiddenReading', () => {
  const inlines: Inline[] = [
    { type: 'text', text: 'the end: ', marks: {} },
    { type: 'spoiler', text: 'he dies', reason: '' },
  ];

  it('a line with a hidden spoiler should read its label, never its words', () =>
    expect(hiddenReading(inlines, '$a', '0', NONE_REVEALED, 'Spoiler')).toBe('the end: Spoiler'));

  it('a line whose spoilers are shown should read as drawn', () => {
    const revealed = revealSpoiler(NONE_REVEALED, inlineSpoilerId('$a', '0', 1, 'he dies'));
    expect(hiddenReading(inlines, '$a', '0', revealed, 'Spoiler')).toBeNull();
  });

  it('a line without a spoiler should read as drawn', () =>
    expect(hiddenReading(inlines.slice(0, 1), '$a', '0', NONE_REVEALED, 'Spoiler')).toBeNull());

  it('a spoiler shown in another message should stay hidden here', () => {
    const revealed = revealSpoiler(NONE_REVEALED, inlineSpoilerId('$b', '0', 1, 'he dies'));
    expect(hiddenReading(inlines, '$a', '0', revealed, 'Spoiler')).toBe('the end: Spoiler');
  });
});
