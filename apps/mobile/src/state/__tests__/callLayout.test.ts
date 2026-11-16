// The desktop's `state::call_layout` tests, one for one.

import { ASPECT, camera, type CallLayout, MIN_HEIGHT, MIN_WIDTH, plan, screen, type TileGeometry } from '../callLayout';

const AUTO: CallLayout = { type: 'Auto' };
const GRID: CallLayout = { type: 'Grid' };

function cameras(count: number): TileGeometry[] {
  return Array.from({ length: count }, (_, i) => ({ key: camera(String(i)), aspect: ASPECT }));
}

describe('plan', () => {
  it('a grid should use the available rectangle', () => {
    const tiles = cameras(6);
    const wide = plan(tiles, [1000, 300], GRID, null, 0, 0.24, false);
    const tall = plan(tiles, [400, 900], GRID, null, 0, 0.24, false);
    expect(wide.tiles[0]!.rect[1]).toBe(wide.tiles[1]!.rect[1]);
    expect(tall.tiles[0]!.rect[1]).not.toBe(tall.tiles[1]!.rect[1]);
  });

  it('an incomplete row should keep the other tiles size', () => {
    const result = plan(cameras(5), [1000, 500], GRID, null, 0, 0.24, false);
    const first = result.tiles[0]!.rect;
    expect(result.tiles.every((tile) => tile.rect[2] === first[2] && tile.rect[3] === first[3])).toBe(true);
  });

  it('layouts should keep non-inset tiles inside the viewport and apart', () => {
    const sizes: Array<[number, number]> = [
      [1, 1],
      [200, 120],
      [360, 400],
      [1000, 300],
      [1800, 900],
    ];
    for (const size of sizes) {
      for (const count of [0, 1, 2, 3, 5, 9, 50]) {
        const tiles = [...cameras(count), { key: screen('share'), aspect: 9 / 16 }];
        for (const layout of [AUTO, GRID, { type: 'Focus', pin: null } as CallLayout]) {
          const result = plan(tiles, size, layout, null, 100, 0.24, false);
          result.tiles.forEach((tile, i) => {
            const [x, y, w, h] = tile.rect;
            expect(x >= -0.01 && y >= -0.01 && w >= 0 && h >= 0 && x + w <= size[0] + 0.01 && y + h <= size[1] + 0.01).toBe(true);
            for (const other of result.tiles.slice(i + 1)) {
              const [ox, oy, ow, oh] = other.rect;
              expect(
                tile.inset ||
                  other.inset ||
                  w === 0 ||
                  h === 0 ||
                  ow === 0 ||
                  oh === 0 ||
                  x + w <= ox + 0.01 ||
                  ox + ow <= x + 0.01 ||
                  y + h <= oy + 0.01 ||
                  oy + oh <= y + 0.01,
              ).toBe(true);
            }
          });
        }
      }
    }
  });

  it('a crowded call should page before tiles become unreadable', () => {
    const result = plan(cameras(40), [800, 450], GRID, null, 0, 0.24, false);
    expect(result.pages).toBeGreaterThan(1);
    expect(result.tiles.every((tile) => tile.rect[2] >= MIN_WIDTH && tile.rect[3] >= MIN_HEIGHT)).toBe(true);
  });

  it('an out of range page should show the last page', () => {
    const result = plan(cameras(40), [800, 450], GRID, null, Number.MAX_SAFE_INTEGER, 0.24, false);
    expect(result.page).toBe(result.pages - 1);
  });

  it('a shared screen should keep its native aspect', () => {
    const result = plan([{ key: screen('a'), aspect: 9 / 16 }], [1000, 500], AUTO, null, 0, 0.24, false);
    expect(Math.abs(result.tiles[0]!.rect[2] / result.tiles[0]!.rect[3] - 9 / 16)).toBeLessThan(0.001);
  });

  it('a narrow share view should put the participants below', () => {
    const tiles = [...cameras(5), { key: screen('a'), aspect: ASPECT }];
    const result = plan(tiles, [400, 500], AUTO, null, 0, 0.24, false);
    expect([result.divider !== null, result.stripBeside]).toEqual([true, false]);
  });

  it('a wide share view should put the participants beside it', () => {
    const tiles = [...cameras(5), { key: screen('a'), aspect: ASPECT }];
    const result = plan(tiles, [1000, 500], AUTO, null, 0, 0.24, false);
    expect([result.divider !== null, result.stripBeside]).toEqual([true, true]);
  });

  it('a grid should allow shared screens to be compared', () => {
    const tiles = ['a', 'b'].map((key) => ({ key: screen(key), aspect: ASPECT }));
    expect(plan(tiles, [1000, 500], GRID, null, 0, 0.24, false).tiles).toHaveLength(2);
  });

  it('a pin should override the speaker', () => {
    const key = camera('2');
    const result = plan(cameras(5), [1000, 500], { type: 'Focus', pin: key }, '1', 0, 0.24, false);
    expect(result.tiles[0]!.key).toEqual(key);
  });

  it('a direct call should keep a small self-view', () => {
    const tiles = [
      { key: camera('remote'), aspect: ASPECT },
      { key: camera(''), aspect: ASPECT },
    ];
    const result = plan(tiles, [800, 500], AUTO, null, 0, 0.24, false);
    expect(result.tiles[1]!.inset).toBe(true);
    expect(result.tiles[1]!.rect[2]).toBeLessThan(result.tiles[0]!.rect[2] / 2);
  });

  it('a compact voice call should fit several faces', () => {
    expect(plan(cameras(4), [360, 100], AUTO, null, 0, 0.24, true).tiles.length).toBeGreaterThanOrEqual(2);
  });

  it('hiding self-view should keep two remote participants in a gallery', () => {
    const result = plan(cameras(2), [800, 450], AUTO, null, 0, 0.24, false);
    expect(result.divider).toBeNull();
    expect(result.tiles[0]!.rect.slice(2)).toEqual(result.tiles[1]!.rect.slice(2));
  });

  it('a phone in portrait should put the participant strip below a shared screen', () => {
    const tiles = [...cameras(3), { key: screen('a'), aspect: ASPECT }];
    const result = plan(tiles, [390, 600], AUTO, null, 0, 0.24, false);
    expect([result.stripBeside, result.tiles[0]!.key]).toEqual([false, screen('a')]);
  });
});
