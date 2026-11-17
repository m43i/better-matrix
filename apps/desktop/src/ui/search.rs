//! Fuzzy match highlights shared by the top bar's result kinds.

use super::theme::{Palette, font};
use rune::{El, Span};

pub fn highlighted(value: &str, query: &str, p: &Palette) -> El {
    let positions = matrix_core::search::fuzzy_match(value, query).map_or_else(Vec::new, |m| m.positions);
    highlights(value, &positions, p)
}

pub fn highlights(value: &str, positions: &[usize], p: &Palette) -> El {
    let spans = value
        .char_indices()
        .enumerate()
        .filter(|(i, _)| positions.binary_search(i).is_ok())
        .map(|(_, (byte, c))| Span::new(byte, byte + c.len_utf8()).color(p.accent).weight(700.0))
        .collect();
    rune::rich(value.to_owned(), spans).font(font::BODY).color(p.text).truncate()
}
