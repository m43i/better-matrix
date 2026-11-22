import { NONE_REVEALED, revealSpoiler, spoilerId } from '../spoilers';

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
