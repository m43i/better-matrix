// The app's logo mark, the desktop's `icons::LOGO`: a sphere of dots with a speech tail, drawn
// with views on its 24-unit grid so it needs no SVG renderer.

import { View } from 'react-native';

/** Each dot's centre and radius on the 24×24 grid. */
const DOTS: ReadonlyArray<readonly [number, number, number]> = [
  [6.2, 4.98, 0.59], [9.1, 4.98, 0.67], [12, 4.98, 0.7], [14.9, 4.98, 0.67], [17.8, 4.98, 0.59],
  [4.75, 7.49, 0.62], [7.65, 7.49, 0.74], [10.55, 7.49, 0.82], [13.45, 7.49, 0.82], [16.35, 7.49, 0.74], [19.25, 7.49, 0.62],
  [3.3, 10, 0.58], [6.2, 10, 0.72], [9.1, 10, 1.25], [12, 10, 1.25], [14.9, 10, 1.25], [17.8, 10, 0.72], [20.7, 10, 0.58],
  [4.75, 12.51, 0.62], [7.65, 12.51, 0.74], [10.55, 12.51, 0.82], [13.45, 12.51, 0.82], [16.35, 12.51, 0.74], [19.25, 12.51, 0.62],
  [6.2, 15.02, 0.59], [9.1, 15.02, 0.67], [12, 15.02, 0.7], [14.9, 15.02, 0.67], [17.8, 15.02, 0.59],
  [6.9, 17.4, 1.05], [5.3, 19.3, 0.7],
];

export function Logo({ size, color }: { size: number; color: string }) {
  const unit = size / 24;
  return (
    <View style={{ width: size, height: size }} accessibilityElementsHidden importantForAccessibility="no-hide-descendants">
      {DOTS.map(([x, y, r]) => (
        <View
          key={`${x}-${y}`}
          style={{
            position: 'absolute',
            left: (x - r) * unit,
            top: (y - r) * unit,
            width: 2 * r * unit,
            height: 2 * r * unit,
            borderRadius: r * unit,
            backgroundColor: color,
          }}
        />
      ))}
    </View>
  );
}
