import vectors from '../../../../../packages/matrix-core/src/math.json';
import { MOST_INPUT, MOST_OUTPUT, mathToUnicode } from '../math';

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

  it('a formula longer than the bound should show as its LaTeX', () => {
    const latex = ` ${'\\alpha'.repeat(MOST_INPUT)} `;

    expect(mathToUnicode(latex)).toBe(latex.trim());
  });

  it('a formula whose accents nest should stack their marks on the letter', () =>
    expect(mathToUnicode('\\hat{\\vec{a b}}')).toBe('a⃗̂ b⃗̂'));

  it('a formula making too much output should show as its LaTeX', () => {
    // Short enough to read, but each accent adds a mark to every letter.
    const latex = `${'\\vec{'.repeat(15)}${'x'.repeat(1900)}${'}'.repeat(15)}`;

    expect(mathToUnicode(latex)).toBe(latex);
  });

  it('a formula whose last part takes its output past the bound should show as its LaTeX', () => {
    // Each half reads within the bound, the two together past it, and nothing follows the
    // second to notice.
    const half = `\\overline{\\overline{\\overline{${'x'.repeat(600)}}}}`;
    const bytes = new TextEncoder().encode(mathToUnicode(half)).length;

    expect([bytes <= MOST_OUTPUT, 2 * bytes > MOST_OUTPUT, mathToUnicode(half + half)]).toEqual([
      true,
      true,
      half + half,
    ]);
  });
});
