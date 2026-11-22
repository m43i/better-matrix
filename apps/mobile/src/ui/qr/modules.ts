// A QR code's dark modules as runs along each row, so a code draws as a few hundred views
// rather than one per module.

import type { QrImage } from '@better-matrix/matrix-native/types';

/** Light modules around the code, as the QR standard asks for scanners to find it. */
export const QUIET_ZONE = 4;

/** Dark modules side by side in one row: `length` of them from column `x` of row `y`. */
export interface Run {
  x: number;
  y: number;
  length: number;
}

/** The code's dark modules, row by row, as runs. */
export function darkRuns(qr: QrImage): Run[] {
  const runs: Run[] = [];

  for (let y = 0; y < qr.width; y++) {
    let x = 0;

    while (x < qr.width) {
      if (!qr.dark[y * qr.width + x]) {
        x++;
        continue;
      }

      const start = x;

      while (x < qr.width && qr.dark[y * qr.width + x]) x++;
      runs.push({ x: start, y, length: x - start });
    }
  }

  return runs;
}

/** The size of one module for a code drawn `size` across with its quiet zone, in whole pixels. */
export function moduleSize(qr: QrImage, size: number, pixelRatio: number): number {
  const modules = qr.width + 2 * QUIET_ZONE;

  return Math.max(1, Math.floor((size / modules) * pixelRatio)) / pixelRatio;
}
