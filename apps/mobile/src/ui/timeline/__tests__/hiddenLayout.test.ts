import { FULL_HEIGHT, hiddenLayout, minTap, tapHeight } from '../hiddenLayout';

describe('hidden media’s placeholder', () => {
  it('a tall box should show the icon, what it hides and Show', () => {
    expect(hiddenLayout(200)).toBe('full');
  });

  it('a short box should show Show alone, so nothing clips', () => {
    expect(hiddenLayout(FULL_HEIGHT - 1)).toBe('show');
  });

  it('a link card’s small picture should show the Show label', () => {
    expect(hiddenLayout(64)).toBe('show');
  });

  it('a box shorter than a finger should sit in a tap area of the platform’s least size', () => {
    expect([tapHeight(30, minTap('ios')), tapHeight(30, minTap('android'))]).toEqual([44, 48]);
  });

  it('a box tall enough to tap should be its own tap area', () => {
    expect(tapHeight(120, minTap('ios'))).toBe(120);
  });
});
