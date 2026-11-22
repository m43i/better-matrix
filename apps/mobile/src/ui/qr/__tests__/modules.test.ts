import { darkRuns, moduleSize } from '../modules';

describe('darkRuns', () => {
  it('should join dark modules side by side into one run', () => {
    const qr = { width: 3, dark: [true, true, false, false, false, false, false, true, true] };
    expect(darkRuns(qr)).toEqual([
      { x: 0, y: 0, length: 2 },
      { x: 1, y: 2, length: 2 },
    ]);
  });

  it('should start a new run after a light module', () => {
    const qr = { width: 3, dark: [true, false, true, false, false, false, false, false, false] };
    expect(darkRuns(qr).map((run) => run.x)).toEqual([0, 2]);
  });
});

describe('moduleSize', () => {
  it('should fit the code and its quiet zone in whole device pixels', () => {
    // 21 modules and 8 of quiet zone in 200 points at 3x: 600 / 29 = 20.6 → 20 pixels.
    expect(moduleSize({ width: 21, dark: [] }, 200, 3)).toBeCloseTo(20 / 3);
  });
});
