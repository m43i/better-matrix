// Call geometry, independent of React Native and media delivery: the desktop's
// `state::call_layout`, ported line for line (ADR 0009). The same plan places the stage's tiles
// and decides which videos get a native view, so a tile that isn't planned converts no frames.

const GAP = 12;
export const ASPECT = 16 / 9;
export const MIN_WIDTH = 160;
export const MIN_HEIGHT = 90;
const MAX_TILES = 25;

/** A participant's surface, independent of a camera restarting with another video ID. */
export interface CallTile {
  /** The participant key; "" for the user. */
  participant: string;
  /** A shared screen, else the camera (or face). */
  screen: boolean;
}

export const camera = (participant: string): CallTile => ({ participant, screen: false });
export const screen = (participant: string): CallTile => ({ participant, screen: true });

/** The tile's identity as a string: element keys and per-tile preferences. */
export function tileId(tile: CallTile): string {
  return `${tile.screen ? 'screen' : 'camera'}:${tile.participant}`;
}

export function sameTile(a: CallTile, b: CallTile): boolean {
  return a.participant === b.participant && a.screen === b.screen;
}

/** Focus without a pin follows the speaker; a pin overrides automatic selection locally. */
export type CallLayout = { type: 'Auto' } | { type: 'Grid' } | { type: 'Focus'; pin: CallTile | null };

export interface TileGeometry {
  key: CallTile;
  aspect: number;
}

/** `[x, y, width, height]` in points of the tile viewport. */
export type Rect = [number, number, number, number];

export interface PlacedTile {
  key: CallTile;
  rect: Rect;
  /** Drawn over another tile (the one-to-one self-view). */
  inset: boolean;
}

export interface CallPlan {
  tiles: PlacedTile[];
  page: number;
  pages: number;
  /** The draggable boundary between the main view and the participant strip. */
  divider: Rect | null;
  stripBeside: boolean;
}

function emptyPlan(): CallPlan {
  return { tiles: [], page: 0, pages: 0, divider: null, stripBeside: false };
}

export function singlePlan(key: CallTile, size: [number, number], aspect: number): CallPlan {
  return { ...emptyPlan(), tiles: [{ key, rect: fitted([0, 0, size[0], size[1]], aspect), inset: false }], pages: 1 };
}

/** The best equal-cell grid: its column count and cell size. */
function gridShape(count: number, bounds: Rect, aspect: number): [number, [number, number]] {
  const [, , w, h] = bounds;
  let best: [number, [number, number]] = [1, [0, 0]];
  for (let columns = 1; columns <= count; columns++) {
    const rows = Math.ceil(count / columns);
    let width = Math.max((w - GAP * (columns - 1)) / columns, 0);
    const height = Math.max((h - GAP * (rows - 1)) / rows, 0);
    width = Math.min(width, height * aspect);
    const fittedHeight = width / aspect;
    if (width * fittedHeight > best[1][0] * best[1][1]) best = [columns, [width, fittedHeight]];
  }
  return best;
}

/** A bounded grid with equally sized cells and a centred incomplete last row. */
function cells(count: number, bounds: Rect, aspect: number): Rect[] {
  if (count === 0) return [];
  const [x, y, w, h] = bounds;
  const [columns, [width, height]] = gridShape(count, bounds, aspect);
  const rows = Math.ceil(count / columns);
  const top = y + (h - rows * height - (rows - 1) * GAP) / 2;
  return Array.from({ length: count }, (_, i) => {
    const row = Math.floor(i / columns);
    const n = Math.min(columns, count - row * columns);
    const left = x + (w - n * width - (n - 1) * GAP) / 2;
    return [left + (i % columns) * (width + GAP), top + row * (height + GAP), width, height];
  });
}

function gallery(tiles: readonly TileGeometry[], bounds: Rect, page: number, voice: boolean): CallPlan {
  if (tiles.length === 0) return emptyPlan();
  const aspect = voice ? 1.2 : ASPECT;
  const minimum = voice ? [104, 80] : [MIN_WIDTH, MIN_HEIGHT];
  let capacity = 1;
  for (let count = Math.min(tiles.length, MAX_TILES); count >= 1; count--) {
    const [, [w, h]] = gridShape(count, bounds, aspect);
    if (w >= (minimum[0] ?? 0) && h >= (minimum[1] ?? 0)) {
      capacity = count;
      break;
    }
  }
  const pages = Math.ceil(tiles.length / capacity);
  const shownPage = Math.min(page, pages - 1);
  const shown = tiles.slice(shownPage * capacity, Math.min(tiles.length, (shownPage + 1) * capacity));
  const rects = cells(shown.length, bounds, aspect);
  return {
    ...emptyPlan(),
    tiles: shown.flatMap((tile, i) => {
      const rect = rects[i];
      return rect ? [{ key: tile.key, rect, inset: false }] : [];
    }),
    page: shownPage,
    pages,
  };
}

function fitted(bounds: Rect, aspect: number): Rect {
  const [x, y, w, h] = bounds;
  const shape = Number.isFinite(aspect) && aspect > 0 ? aspect : ASPECT;
  const width = Math.min(w, h * shape);
  const height = width / shape;
  return [x + (w - width) / 2, y + (h - height) / 2, width, height];
}

const isRemoteCamera = (tile: TileGeometry) => !tile.key.screen && tile.key.participant !== '';

/** All sizes are points in the tile viewport; controls and paging live outside this rectangle. */
export function plan(
  tiles: readonly TileGeometry[],
  size: [number, number],
  layout: CallLayout,
  speaker: string | null,
  page: number,
  strip: number,
  voice: boolean,
): CallPlan {
  const [w, h] = size;
  if (!Number.isFinite(w) || !Number.isFinite(h) || w <= 0 || h <= 0) return emptyPlan();
  const bounds: Rect = [0, 0, w, h];
  if (voice || layout.type === 'Grid') return gallery(tiles, bounds, page, voice);
  const shared = tiles.find((tile) => tile.key.screen);
  const remote = tiles.find(isRemoteCamera);
  let main: TileGeometry | undefined;
  if (layout.type === 'Focus') {
    const pin = layout.pin;
    main =
      (pin ? tiles.find((tile) => sameTile(tile.key, pin)) : undefined) ??
      (speaker !== null ? tiles.find((tile) => !tile.key.screen && tile.key.participant === speaker) : undefined) ??
      remote ??
      shared ??
      tiles[0];
  } else {
    main = shared ?? (tiles.filter(isRemoteCamera).length === 1 ? remote : undefined);
  }
  if (!main) return gallery(tiles, bounds, page, false);
  const focused = main;
  const others = tiles.filter((tile) => !sameTile(tile.key, focused.key));
  if (others.length === 0) return singlePlan(focused.key, size, focused.aspect);
  // A one-to-one camera call keeps a small self-view over the remote participant.
  const [only] = others;
  if (!focused.key.screen && others.length === 1 && only && sameTile(only.key, camera(''))) {
    const width = Math.min(Math.min(Math.max(w * 0.24, 80), 200), w * 0.4, h * 0.4 * ASPECT);
    const height = width / ASPECT;
    return {
      ...emptyPlan(),
      tiles: [
        { key: focused.key, rect: fitted(bounds, focused.aspect), inset: false },
        { key: only.key, rect: [Math.max(w - width - GAP, 0), Math.max(h - height - GAP, 0), width, height], inset: true },
      ],
      pages: 1,
    };
  }
  const beside = w / h >= 1.5 && w >= 640;
  const fraction = Math.min(Math.max(strip, 0.18), 0.4);
  let mainBounds: Rect;
  let stripBounds: Rect;
  let divider: Rect;
  if (beside) {
    const width = Math.min(Math.max(w * fraction, 160), w * 0.4);
    const edge = w - width - GAP;
    mainBounds = [0, 0, edge, h];
    stripBounds = [w - width, 0, width, h];
    divider = [edge, 0, GAP, h];
  } else {
    const height = Math.min(Math.max(h * fraction, 90), h * 0.4);
    const edge = Math.max(h - height - GAP, 0);
    mainBounds = [0, 0, w, edge];
    stripBounds = [0, h - height, w, height];
    divider = [0, edge, w, GAP];
  }
  const result = gallery(others, stripBounds, page, false);
  result.tiles.unshift({ key: focused.key, rect: fitted(mainBounds, focused.aspect), inset: false });
  result.divider = divider;
  result.stripBeside = beside;
  return result;
}
