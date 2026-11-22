import vectors from '../../../../../packages/matrix-core/src/math.json';
import { mathToUnicode } from '../math';

describe('mathToUnicode', () => {
  // The engine reads maths against the same vectors (packages/matrix-core/src/math.rs).
  it.each(vectors.map((v) => [v.latex, v.unicode] as const))(
    'should read %j as the engine does',
    (latex, unicode) => {
      expect(mathToUnicode(latex)).toBe(unicode);
    },
  );

  it('should be tested against more than a handful of vectors', () =>
    expect(vectors.length).toBeGreaterThan(15));
});
