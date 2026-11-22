import { FULL_SIDE, hiddenLayout, minTap, SHOW_WIDTH, tapArea } from '../hiddenLayout';

describe('hidden media’s placeholder', () => {
  it('a large box should show the icon, what it hides and Show', () => {
    expect(hiddenLayout(200, 200)).toBe('full');
  });

  it('a wide, short box should show Show alone in it, so nothing clips', () => {
    expect(hiddenLayout(240, FULL_SIDE - 1)).toBe('show');
  });

  it('a link card’s small picture should put Show under it', () => {
    expect(hiddenLayout(64, 64)).toBe('outside');
  });

  it('a box too narrow for the word should put Show under it', () => {
    expect(hiddenLayout(SHOW_WIDTH - 1, 60)).toBe('outside');
  });

  it('a tiny sticker should sit in a tap area of the platform’s least size on both axes', () => {
    expect([tapArea(32, 32, minTap('ios')), tapArea(32, 32, minTap('android'))]).toEqual([
      { minWidth: 44, minHeight: 44 },
      { minWidth: 48, minHeight: 48 },
    ]);
  });

  it('a box large enough to tap should be its own tap area', () => {
    expect(tapArea(120, 90, minTap('ios'))).toEqual({ minWidth: 120, minHeight: 90 });
  });
});
