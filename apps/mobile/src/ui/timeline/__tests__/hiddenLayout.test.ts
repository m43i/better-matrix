import { FULL_HEIGHT, hiddenLayout, minTap, tapSlop } from '../hiddenLayout';

describe('hidden media’s placeholder', () => {
  it('a tall box should show the icon, what it hides and Show', () => {
    expect(hiddenLayout(200, false)).toBe('full');
  });

  it('a short box should show Show alone, so nothing clips', () => {
    expect(hiddenLayout(FULL_HEIGHT - 1, false)).toBe('show');
  });

  it('a link card’s small picture should keep the icon alone', () => {
    expect(hiddenLayout(64, true)).toBe('icon');
  });

  it('a box shorter than a finger should reach the platform’s least tap size', () => {
    expect([tapSlop(30, minTap('ios')), tapSlop(30, minTap('android'))]).toEqual([7, 9]);
  });

  it('a box tall enough to tap should not reach past itself', () => {
    expect(tapSlop(120, minTap('ios'))).toBe(0);
  });
});
