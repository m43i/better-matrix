# Rune text renderer

This is `crates/rune-text` from Rune at `259acd7512a0eebcb44627a14ba51e7e94194b97`, patched
through Cargo. The crate exposes its bundled fonts and an app-selected emoji fallback list.
For emoji graphemes it tries that list ahead of the system emoji family, keeping the system
fallback for gaps in the chosen font. A change goes through Rune's existing family invalidation
path, including measurements, layouts and editable paragraphs.

When updating the Rune pin, refresh this crate from that revision and reapply the fallback
changes in `src/fonts/families.rs`, `src/layout/shape.rs`, `src/lib.rs` and `src/measure.rs`.
The standalone manifest uses Git dependencies for Rune's sibling crates. Its font assets come
from `crates/rune/assets` at the same revision.
