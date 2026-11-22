// Maths in messages: LaTeX (`data-mx-maths`) shown as one line of Unicode text, ported line for
// line from the engine's `packages/matrix-core/src/math.rs` and checked against its vectors
// (`math.json`), so every app reads a formula the same. Greek letters and symbols by name, `x^2`
// as `x²`, `\frac{a}{b}` as `a/b`, `\sqrt{x}` as `√x`, `\mathbb{R}` as `ℝ`; what has no Unicode
// form (`x^{q}`) keeps a readable fallback (`x^(q)`), and unknown commands keep their name.

/** A LaTeX formula as one line of Unicode text. */
export function mathToUnicode(latex: string): string {
  const parser = new Parser(Array.from(latex));
  const out = parser.sequence(null);
  // Runs of spaces from `\,`, `\quad` and the source's own collapse to one.
  let collapsed = '';

  for (const c of out.trim()) {
    if (!(c === ' ' && collapsed.endsWith(' '))) collapsed += c;
  }

  return collapsed;
}

const WHITESPACE = /\s/u;

const ALPHANUMERIC = /[\p{L}\p{N}]/u;

const ASCII_LETTER = /[A-Za-z]/;

class Parser {
  private at = 0;

  constructor(private readonly chars: string[]) {}

  private peek(): string | undefined {
    return this.chars[this.at];
  }

  /** Everything up to `end` (a closing brace or bracket) or the end of the formula. */
  sequence(end: string | null): string {
    let out = '';

    for (let c = this.peek(); c !== undefined; c = this.peek()) {
      if (c === end) {
        this.at += 1;

        return out;
      }

      if (c === '^' || c === '_') {
        this.at += 1;
        const script = this.argument();
        out += scripted(script, c === '^');
      } else {
        out += this.atom();
      }
    }

    return out;
  }

  /** One thing: a group, a command with its arguments, or a character. */
  private atom(): string {
    const c = this.peek();

    if (c === undefined) return '';
    this.at += 1;

    if (c === '{') return this.sequence('}');

    if (c === '\\') return this.command();

    if (c === '~' || c === '&' || WHITESPACE.test(c)) return ' ';

    return c;
  }

  /** A command's argument: a group, or the next single thing. */
  private argument(): string {
    while (this.peek() !== undefined && WHITESPACE.test(this.peek() ?? '')) this.at += 1;

    return this.atom();
  }

  /** An optional `[...]` argument. */
  private optional(): string | null {
    if (this.peek() !== '[') return null;
    this.at += 1;

    return this.sequence(']');
  }

  /** Each character of the next argument followed by `mark`. */
  private marked(mark: string): string {
    return Array.from(this.argument())
      .map((c) => c + mark)
      .join('');
  }

  private command(): string {
    const start = this.at;

    while (ASCII_LETTER.test(this.peek() ?? '')) this.at += 1;

    if (this.at === start) {
      // A one-character command: `\{`, `\,`, `\\`.
      const c = this.peek();

      if (c === undefined) return '\\';
      this.at += 1;

      return [',', ';', ':', '!', ' ', '\\'].includes(c) ? ' ' : c;
    }

    const name = this.chars.slice(start, this.at).join('');

    switch (name) {
      case 'frac':
      case 'dfrac':
      case 'tfrac': {
        const top = this.argument();
        const bottom = this.argument();

        return `${wrapped(top)}/${wrapped(bottom)}`;
      }

      case 'sqrt': {
        const index = this.optional()?.trim() ?? null;
        const radicand = this.argument();

        const root =
          index === null || index === '2'
            ? '√'
            : index === '3'
              ? '∛'
              : index === '4'
                ? '∜'
                : `${scripted(index, true)}√`;

        return root + wrapped(radicand);
      }

      case 'text':
      case 'mathrm':
      case 'textrm':
      case 'mathit':
      case 'textit':
      case 'mathbf':
      case 'textbf':
      case 'operatorname':
      case 'mbox':
        return this.argument();
      case 'mathbb':
        return Array.from(this.argument()).map(doubleStruck).join('');
      case 'left':
      case 'right':
      case 'big':
      case 'Big':
      case 'bigg':
      case 'Bigg':
      case 'displaystyle':
      case 'limits':
        return '';
      case 'quad':
      case 'qquad':
        return ' ';
      case 'begin':
      case 'end':
        // Environments (`matrix`, `cases`) show their contents.
        this.argument();

        return '';
      case 'overline':
      case 'bar':
        return this.marked('̅');
      case 'hat':
        return this.marked('̂');
      case 'vec':
        return this.marked('⃗');
      case 'dot':
        return this.marked('̇');
      case 'tilde':
        return this.marked('̃');
      default:
        // Kept, set apart from what follows it.
        return SYMBOLS.get(name) ?? `\\${name} `;
    }
  }
}

/** `text` in parentheses when it is more than one thing, so `a+b` over `c` reads `(a+b)/c`. */
function wrapped(raw: string): string {
  const text = raw.trim();
  const chars = Array.from(text);

  if (chars.length <= 1 || chars.every((c) => ALPHANUMERIC.test(c) || c === '.')) return text;

  return `(${text})`;
}

/**
 * `text` raised or lowered: in Unicode's superscript or subscript letters when every character
 * has one, else `^(text)` or `_(text)`.
 */
function scripted(raw: string, up: boolean): string {
  const text = raw.trim();
  const chars = Array.from(text);
  const table = up ? SUPERSCRIPT : SUBSCRIPT;
  const mapped = chars.map((c) => table.get(c));
  const sign = up ? '^' : '_';

  if (chars.length > 0 && mapped.every((c) => c !== undefined)) return mapped.join('');

  if (chars.length === 1) return `${sign}${text}`;

  return `${sign}(${text})`;
}

function pairs(from: string, to: string): Map<string, string> {
  const keys = Array.from(from);
  const values = Array.from(to);

  return new Map(keys.map((key, i) => [key, values[i] ?? key]));
}

const SUPERSCRIPT = new Map([
  ...pairs('0123456789+-−=()ni', '⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁻⁼⁽⁾ⁿⁱ'),
  ...pairs('abcdefghjklmoprstuvwxyzT′*', 'ᵃᵇᶜᵈᵉᶠᵍʰʲᵏˡᵐᵒᵖʳˢᵗᵘᵛʷˣʸᶻᵀ′*'),
]);

const SUBSCRIPT = pairs('0123456789+-−=()aehijklmnoprstuvx', '₀₁₂₃₄₅₆₇₈₉₊₋₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ');

const DOUBLE_STRUCK = pairs('CHNPQRZ', 'ℂℍℕℙℚℝℤ');

/** A letter in double-struck type (`\mathbb{R}` → `ℝ`); others stay. */
function doubleStruck(c: string): string {
  const special = DOUBLE_STRUCK.get(c);

  if (special) return special;

  if (c >= 'A' && c <= 'Z') return String.fromCodePoint(0x1d538 + c.charCodeAt(0) - 65);

  return c;
}

const FUNCTIONS = [
  'sin',
  'cos',
  'tan',
  'log',
  'ln',
  'exp',
  'lim',
  'max',
  'min',
  'det',
  'sup',
  'inf',
  'arg',
  'deg',
  'dim',
  'gcd',
  'mod',
  'sinh',
  'cosh',
  'tanh',
  'arcsin',
  'arccos',
  'arctan',
  'sec',
  'csc',
  'cot',
  'ker',
  'Pr',
];

/** The symbol each command names; named functions are written out (`\sin` → `sin`). */
const SYMBOLS = new Map<string, string>([
  ['alpha', 'α'],
  ['beta', 'β'],
  ['gamma', 'γ'],
  ['delta', 'δ'],
  ['epsilon', 'ϵ'],
  ['varepsilon', 'ε'],
  ['zeta', 'ζ'],
  ['eta', 'η'],
  ['theta', 'θ'],
  ['vartheta', 'ϑ'],
  ['iota', 'ι'],
  ['kappa', 'κ'],
  ['lambda', 'λ'],
  ['mu', 'μ'],
  ['nu', 'ν'],
  ['xi', 'ξ'],
  ['pi', 'π'],
  ['varpi', 'ϖ'],
  ['rho', 'ρ'],
  ['sigma', 'σ'],
  ['varsigma', 'ς'],
  ['tau', 'τ'],
  ['upsilon', 'υ'],
  ['phi', 'ϕ'],
  ['varphi', 'φ'],
  ['chi', 'χ'],
  ['psi', 'ψ'],
  ['omega', 'ω'],
  ['Gamma', 'Γ'],
  ['Delta', 'Δ'],
  ['Theta', 'Θ'],
  ['Lambda', 'Λ'],
  ['Xi', 'Ξ'],
  ['Pi', 'Π'],
  ['Sigma', 'Σ'],
  ['Upsilon', 'Υ'],
  ['Phi', 'Φ'],
  ['Psi', 'Ψ'],
  ['Omega', 'Ω'],
  ['times', '×'],
  ['cdot', '·'],
  ['pm', '±'],
  ['mp', '∓'],
  ['div', '÷'],
  ['ast', '∗'],
  ['star', '⋆'],
  ['circ', '∘'],
  ['bullet', '•'],
  ['leq', '≤'],
  ['le', '≤'],
  ['geq', '≥'],
  ['ge', '≥'],
  ['neq', '≠'],
  ['ne', '≠'],
  ['ll', '≪'],
  ['gg', '≫'],
  ['approx', '≈'],
  ['equiv', '≡'],
  ['cong', '≅'],
  ['sim', '∼'],
  ['simeq', '≃'],
  ['propto', '∝'],
  ['infty', '∞'],
  ['partial', '∂'],
  ['nabla', '∇'],
  ['sum', '∑'],
  ['prod', '∏'],
  ['coprod', '∐'],
  ['int', '∫'],
  ['iint', '∬'],
  ['iiint', '∭'],
  ['oint', '∮'],
  ['in', '∈'],
  ['notin', '∉'],
  ['ni', '∋'],
  ['subset', '⊂'],
  ['subseteq', '⊆'],
  ['supset', '⊃'],
  ['supseteq', '⊇'],
  ['cup', '∪'],
  ['cap', '∩'],
  ['setminus', '∖'],
  ['emptyset', '∅'],
  ['varnothing', '∅'],
  ['forall', '∀'],
  ['exists', '∃'],
  ['nexists', '∄'],
  ['neg', '¬'],
  ['lnot', '¬'],
  ['land', '∧'],
  ['wedge', '∧'],
  ['lor', '∨'],
  ['vee', '∨'],
  ['oplus', '⊕'],
  ['otimes', '⊗'],
  ['to', '→'],
  ['rightarrow', '→'],
  ['leftarrow', '←'],
  ['gets', '←'],
  ['leftrightarrow', '↔'],
  ['Rightarrow', '⇒'],
  ['implies', '⇒'],
  ['Leftarrow', '⇐'],
  ['Leftrightarrow', '⇔'],
  ['iff', '⇔'],
  ['mapsto', '↦'],
  ['uparrow', '↑'],
  ['downarrow', '↓'],
  ['ldots', '…'],
  ['dots', '…'],
  ['cdots', '⋯'],
  ['vdots', '⋮'],
  ['ddots', '⋱'],
  ['degree', '°'],
  ['angle', '∠'],
  ['perp', '⊥'],
  ['parallel', '∥'],
  ['mid', '∣'],
  ['hbar', 'ℏ'],
  ['ell', 'ℓ'],
  ['Re', 'ℜ'],
  ['Im', 'ℑ'],
  ['aleph', 'ℵ'],
  ['wp', '℘'],
  ['prime', '′'],
  ['langle', '⟨'],
  ['rangle', '⟩'],
  ['lfloor', '⌊'],
  ['rfloor', '⌋'],
  ['lceil', '⌈'],
  ['rceil', '⌉'],
  ['vert', '|'],
  ['lvert', '|'],
  ['rvert', '|'],
  ['Vert', '‖'],
  ['lVert', '‖'],
  ['rVert', '‖'],
  ...FUNCTIONS.map((name): [string, string] => [name, name]),
]);
