//! Maths in messages: LaTeX (`data-mx-maths`) shown as Unicode text. Apps draw text, not
//! typeset formulas, so a formula becomes the closest line of Unicode: Greek letters and
//! symbols by name, `x^2` as `x²`, `\frac{a}{b}` as `a/b`, `\sqrt{x}` as `√x`, `\mathbb{R}` as
//! `ℝ`. What has no Unicode form (`x^{q}`) keeps a readable fallback (`x^(q)`), and unknown
//! commands keep their name, so nothing the sender wrote disappears. Every app reads a formula
//! the same: the TypeScript port runs the same vectors (`math.json`).

/// A LaTeX formula as one line of Unicode text.
///
/// ```
/// use matrix_core::math::to_unicode;
/// assert_eq!(to_unicode(r"x^2 + y^2 = r^2"), "x² + y² = r²");
/// assert_eq!(to_unicode(r"\frac{a+b}{2}"), "(a+b)/2");
/// ```
pub fn to_unicode(latex: &str) -> String {
    let chars: Vec<char> = latex.chars().collect();
    let mut parser = Parser { chars: &chars, at: 0 };
    let out = parser.sequence(None);
    // Runs of spaces from `\,`, `\quad` and the source's own collapse to one.
    let mut collapsed = String::with_capacity(out.len());
    for c in out.trim().chars() {
        if !(c == ' ' && collapsed.ends_with(' ')) {
            collapsed.push(c);
        }
    }
    collapsed
}

struct Parser<'a> {
    chars: &'a [char],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    /// Everything up to `end` (a closing brace or bracket) or the end of the formula.
    fn sequence(&mut self, end: Option<char>) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if Some(c) == end {
                self.at += 1;
                return out;
            }
            match c {
                '^' | '_' => {
                    self.at += 1;
                    let script = self.argument();
                    out.push_str(&scripted(&script, c == '^'));
                }
                _ => out.push_str(&self.atom()),
            }
        }
        out
    }

    /// One thing: a group, a command with its arguments, or a character.
    fn atom(&mut self) -> String {
        let Some(c) = self.peek() else { return String::new() };
        self.at += 1;
        match c {
            '{' => self.sequence(Some('}')),
            '\\' => self.command(),
            '~' => " ".to_owned(),
            '&' => " ".to_owned(),
            c if c.is_whitespace() => " ".to_owned(),
            c => c.to_string(),
        }
    }

    /// A command's argument: a group, or the next single thing.
    fn argument(&mut self) -> String {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
        self.atom()
    }

    /// An optional `[...]` argument.
    fn optional(&mut self) -> Option<String> {
        (self.peek() == Some('[')).then(|| {
            self.at += 1;
            self.sequence(Some(']'))
        })
    }

    fn command(&mut self) -> String {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.at += 1;
        }
        if self.at == start {
            // A one-character command: `\{`, `\,`, `\\`.
            let Some(c) = self.peek() else { return "\\".to_owned() };
            self.at += 1;
            return match c {
                ',' | ';' | ':' | '!' | ' ' => " ".to_owned(),
                '\\' => " ".to_owned(),
                c => c.to_string(),
            };
        }
        let name: String = self.chars[start..self.at].iter().collect();
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let (top, bottom) = (self.argument(), self.argument());
                format!("{}/{}", wrapped(&top), wrapped(&bottom))
            }
            "sqrt" => {
                let index = self.optional();
                let radicand = self.argument();
                let root = match index.as_deref().map(str::trim) {
                    None | Some("2") => "√".to_owned(),
                    Some("3") => "∛".to_owned(),
                    Some("4") => "∜".to_owned(),
                    Some(other) => format!("{}√", scripted(other, true)),
                };
                format!("{root}{}", wrapped(&radicand))
            }
            "text" | "mathrm" | "textrm" | "mathit" | "textit" | "mathbf" | "textbf" | "operatorname" | "mbox" => {
                self.argument()
            }
            "mathbb" => self.argument().chars().map(double_struck).collect(),
            "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "displaystyle" | "limits" => String::new(),
            "quad" | "qquad" => " ".to_owned(),
            "begin" | "end" => {
                // Environments (`matrix`, `cases`) show their contents.
                self.argument();
                String::new()
            }
            "overline" | "bar" => self.argument().chars().flat_map(|c| [c, '\u{305}']).collect(),
            "hat" => self.argument().chars().flat_map(|c| [c, '\u{302}']).collect(),
            "vec" => self.argument().chars().flat_map(|c| [c, '\u{20d7}']).collect(),
            "dot" => self.argument().chars().flat_map(|c| [c, '\u{307}']).collect(),
            "tilde" => self.argument().chars().flat_map(|c| [c, '\u{303}']).collect(),
            function if FUNCTIONS.contains(&function) => function.to_owned(),
            other => match symbol(other) {
                Some(symbol) => symbol.to_owned(),
                // Kept, set apart from what follows it.
                None => format!("\\{other} "),
            },
        }
    }
}

/// `text` in parentheses when it is more than one thing, so `a+b` over `c` reads `(a+b)/c`.
fn wrapped(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= 1 || text.chars().all(|c| c.is_alphanumeric() || c == '.') {
        text.to_owned()
    } else {
        format!("({text})")
    }
}

/// `text` raised or lowered: in Unicode's superscript or subscript letters when every
/// character has one, else `^(text)` or `_(text)`.
fn scripted(text: &str, up: bool) -> String {
    let text = text.trim();
    let mapped: Option<String> = text.chars().map(|c| if up { superscript(c) } else { subscript(c) }).collect();
    match mapped {
        Some(mapped) if !mapped.is_empty() => mapped,
        _ if text.chars().count() == 1 => format!("{}{text}", if up { '^' } else { '_' }),
        _ => format!("{}({text})", if up { '^' } else { '_' }),
    }
}

fn superscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' | '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'T' => 'ᵀ',
        '′' => '′',
        '*' => '*',
        _ => return None,
    })
}

fn subscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' | '−' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

/// A letter in double-struck type (`\mathbb{R}` → `ℝ`); others stay.
fn double_struck(c: char) -> char {
    match c {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        c @ 'A'..='Z' => char::from_u32(0x1D538 + (c as u32 - 'A' as u32)).unwrap_or(c),
        c => c,
    }
}

/// The symbol a command names.
fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "times" => "×",
        "cdot" => "·",
        "pm" => "±",
        "mp" => "∓",
        "div" => "÷",
        "ast" => "∗",
        "star" => "⋆",
        "circ" => "∘",
        "bullet" => "•",
        "leq" | "le" => "≤",
        "geq" | "ge" => "≥",
        "neq" | "ne" => "≠",
        "ll" => "≪",
        "gg" => "≫",
        "approx" => "≈",
        "equiv" => "≡",
        "cong" => "≅",
        "sim" => "∼",
        "simeq" => "≃",
        "propto" => "∝",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "sum" => "∑",
        "prod" => "∏",
        "coprod" => "∐",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "emptyset" | "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "nexists" => "∄",
        "neg" | "lnot" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "to" | "rightarrow" => "→",
        "leftarrow" | "gets" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "degree" => "°",
        "angle" => "∠",
        "perp" => "⊥",
        "parallel" => "∥",
        "mid" => "∣",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "wp" => "℘",
        "prime" => "′",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "vert" | "lvert" | "rvert" => "|",
        "Vert" | "lVert" | "rVert" => "‖",
        _ => return None,
    })
}

/// Named functions, written out as they are (`\\sin` → `sin`).
const FUNCTIONS: [&str; 28] = [
    "sin", "cos", "tan", "log", "ln", "exp", "lim", "max", "min", "det", "sup", "inf", "arg", "deg", "dim", "gcd",
    "mod", "sinh", "cosh", "tanh", "arcsin", "arccos", "arctan", "sec", "csc", "cot", "ker", "Pr",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Vector {
        latex: String,
        unicode: String,
    }

    #[test]
    fn every_shared_vector_should_read_the_same() {
        let vectors: Vec<Vector> = serde_json::from_str(include_str!("math.json")).expect("vectors");
        let wrong: Vec<_> = vectors
            .iter()
            .map(|vector| (vector.latex.as_str(), to_unicode(&vector.latex), vector.unicode.as_str()))
            .filter(|(_, read, expected)| read != expected)
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
