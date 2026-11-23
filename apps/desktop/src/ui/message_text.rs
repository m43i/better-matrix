//! Read-only message formatting. Selectable text keeps link hit testing and text selection
//! without making a received message a text field.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use rune::{Block, Color, Cursor, El, InlineIcon, Link, Mark, PillStyle, Span, TextEdit, TextStyle, col, row, text};
use unicode_segmentation::UnicodeSegmentation;

use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{EmojiStyle, MediaCache, PlacedCode, RichText, SPOILER_LINK, State, place_code};

/// What a message's text needs besides its words: whose it is, who reads it, which spoilers
/// they revealed, and the custom emoji's pictures.
#[derive(Clone, Copy)]
pub struct Look<'a> {
    /// The message's event ID: its spoilers' link ids carry it.
    pub event_id: &'a str,
    /// The reader's own user ID: their mentions stand out.
    pub me: &'a str,
    pub revealed: &'a HashSet<String>,
    pub media: &'a MediaCache,
    pub emoji_style: EmojiStyle,
    /// Whether its custom emoji show as pictures: not while the account keeps the message's
    /// media back (ADR 0011), when they read as their short codes.
    pub pictures: bool,
}

/// How far beside a message's text a press still lands on the text: a click or drag that
/// starts just left or right of the words selects them rather than opening the thread. Less
/// than the gap to the sender's picture and the readers' column, which keep their own presses.
pub(super) const REACH: f32 = 8.0;

/// Widens a message text's press target by [`REACH`] on each side without moving its words.
pub fn reach(text: El) -> El {
    text.px(REACH).mx(-REACH)
}

/// How formatted text fits its place: wrapping as a message's body does, or one line cut short.
#[derive(Clone, Copy)]
enum Fit {
    /// Body text in its usual or italic style, wrapping.
    Wrap(TextStyle, Color),
    /// One line in `style` and `color`, ending in "…" when longer than its room.
    Line(TextStyle, Color),
}

pub fn formatted(key: &str, rich: &RichText, look: Look<'_>, p: &Palette) -> El {
    formatted_in(key, rich, look, p.text, p)
}

/// [`formatted`] text whose plain words take `color`.
pub fn formatted_in(key: &str, rich: &RichText, look: Look<'_>, color: Color, p: &Palette) -> El {
    let edit = &rich.edit;
    let fit = Fit::Wrap(font::BODY, color);
    let mut body = col().key(key.to_owned()).w_full().selection_group(true);
    let placed = place_code(edit, &rich.code);
    let mut colors_by_line = code_colors(rich, &placed, p);
    // The code block being gathered: which one it is (its place, or none for code no block was
    // placed on), where it starts, its lines' elements and its code.
    let mut code: Option<(Option<usize>, usize, Vec<El>, String)> = None;
    let finish = |code: Option<(Option<usize>, usize, Vec<El>, String)>| {
        code.map(|(_, start, lines, words)| code_block(key, start, lines, words, p))
    };
    let mut paragraphs = edit.paragraphs().into_iter().peekable();
    let mut numbers = [0usize; 4];
    while let Some(paragraph) = paragraphs.next() {
        let mut range = paragraph.range;
        while paragraphs.peek().is_some_and(|next| next.soft_break) {
            if let Some(next) = paragraphs.next() {
                range.end = next.range.end;
            }
        }
        let block = paragraph.block;
        let line_key = format!("{key}-{}", range.start);
        let place = placed.iter().position(|code| code.first <= paragraph.index && paragraph.index <= code.last);
        // A blank line Rune put between placed blocks belongs to neither: an ordinary line.
        let block =
            if block == Block::Code && !placed.is_empty() && place.is_none() { Block::Paragraph } else { block };
        if block == Block::Code {
            let colors = colors_by_line.remove(&range.start).unwrap_or_default();
            let line =
                row().w_full().px(8.0).child(inline(&line_key, edit, range.clone(), block, colors, look, fit, p));
            match &mut code {
                Some((from, _, lines, words)) if *from == place => {
                    lines.push(line);
                    words.push('\n');
                    words.push_str(&edit.text[range]);
                }
                _ => {
                    body = body.children(finish(code.take()));
                    code = Some((place, range.start, vec![line], edit.text[range].to_owned()));
                }
            }
            continue;
        }
        body = body.children(finish(code.take()));
        let content = inline(&line_key, edit, range, block, Vec::new(), look, fit, p);
        let paragraph = match block {
            Block::Bullet(level) | Block::Numbered(level) => {
                let level = usize::from(level).min(numbers.len() - 1);
                numbers[level + 1..].fill(0);
                let marker = if matches!(block, Block::Numbered(_)) {
                    numbers[level] += 1;
                    format!("{}.", numbers[level])
                } else {
                    numbers[level] = 0;
                    ["•", "◦", "▪"][level.min(2)].to_owned()
                };
                row()
                    .w_full()
                    .pl(level as f32 * 20.0)
                    .gap(6.0)
                    .child(text(marker.clone()).font(font::BODY).color(p.text_muted).w(20.0))
                    .child(content.copy_prefix(format!("{marker} ")))
            }
            Block::Quote => row().w_full().pl(12.0).border_sides([0.0, 0.0, 0.0, 3.0], p.divider).child(content),
            Block::Paragraph | Block::Code => {
                numbers.fill(0);
                content
            }
        };
        body = body.child(paragraph);
    }
    body.children(finish(code))
}

/// A code block: its lines, and a button floating at its top right while it's hovered that
/// copies its code. Its lines keep clear of the button, so it never covers code to select.
/// Room at a code block's right for its copy button: the button and its margin. A block is at
/// least as tall, so the button never hangs below a one-line block.
const COPY_ROOM: f32 = 32.0;

fn code_block(key: &str, start: usize, lines: Vec<El>, code: String, p: &Palette) -> El {
    let copy = super::components::icon_button(&super::icons::COPY, t!("actions.copy_code"), false, p.text_muted, p)
        .key(format!("{key}-copy-code-{start}"))
        .fill(p.surface)
        .border(1.0, p.divider)
        .pin(None, Some(4.0), Some(4.0), None)
        .group_opacity(0.0, 1.0)
        .on_press_tx(move |_: &mut State, tx| tx.copy(code.clone()));
    col()
        .key(format!("{key}-code-{start}"))
        .w_full()
        .pr(COPY_ROOM)
        .min_h(COPY_ROOM + 4.0)
        .fill(p.surface)
        .radius(4.0)
        .hover_group()
        .children(lines)
        .child(copy)
}

/// Formatted text on one line in `style` and `color`, cut short with "…": its blocks (list
/// markers, quotes) are left out, its words keep their marks.
pub fn line(key: &str, edit: &TextEdit, look: Look<'_>, style: TextStyle, color: Color, p: &Palette) -> El {
    inline(key, edit, 0..edit.text.len(), Block::Paragraph, Vec::new(), look, Fit::Line(style, color), p)
}

pub fn plain(key: &str, words: &str, look: Look<'_>, style: TextStyle, color: Color, p: &Palette) -> El {
    let edit = TextEdit::new(words);
    inline(key, &edit, 0..words.len(), Block::Paragraph, Vec::new(), look, Fit::Wrap(style, color), p)
}

/// The colours of `rich`'s code in its blocks' languages: each code line's spans (from the
/// line's start), by where the line starts in the text. Each block is highlighted whole, once
/// (and cached), so a comment or string over several lines colours each of them, and its
/// tokens are walked once alongside its lines.
fn code_colors(rich: &RichText, placed: &[PlacedCode<'_>], p: &Palette) -> HashMap<usize, Vec<Span>> {
    let lines = rich.edit.paragraphs();
    let mut colors = HashMap::new();
    for block in placed {
        let Some(language) = block.language else { continue };
        let (start, end) = (lines[block.first].range.start, lines[block.last].range.end);
        let Some(code) = rich.edit.text.get(start..end) else { continue };
        let found = rune_kit::syntax::highlight(Some(language), code);
        let mut next = 0;
        for line in &lines[block.first..=block.last] {
            let (from, to) = (line.range.start - start, line.range.end - start);
            while found.get(next).is_some_and(|(range, _)| range.end <= from) {
                next += 1;
            }
            // A token running on past the line stays for the next one.
            let on_line: Vec<_> = found[next..]
                .iter()
                .take_while(|(range, _)| range.start < to)
                .map(|(range, paint)| (range.start.max(from) - from..range.end.min(to) - from, *paint))
                .collect();
            colors.insert(line.range.start, p.syntax.spans(&on_line, 0));
        }
    }
    colors
}

#[expect(clippy::too_many_arguments, reason = "one line's text, place and look, as each caller has them")]
fn inline(
    key: &str,
    edit: &TextEdit,
    range: Range<usize>,
    block: Block,
    // A code line's colours in its language.
    colors: Vec<Span>,
    look: Look<'_>,
    fit: Fit,
    p: &Palette,
) -> El {
    let words = &edit.text[range.clone()];
    let mut spans = Vec::new();
    let mut links = Vec::new();
    let mut strikes = Vec::new();
    let mut pills = Vec::new();
    let mut hidden = Vec::new();
    let artwork = if block == Block::Code { Vec::new() } else { illustrations(edit, &range, look, p) };
    // Rune attaches an inline image to the word that follows it. A hair space after
    // the emoji ends that word when another character follows. Keep the authored
    // Unicode in the text, and exclude only the added spaces from copying.
    const SEPARATOR: char = '\u{200a}';
    let separators: Vec<_> = artwork
        .iter()
        .map(|(range, _, _)| range.end)
        .filter(|&end| words[end..].starts_with(|c: char| !c.is_whitespace()))
        .collect();
    let remap = |at: usize, include_at: bool| {
        at + SEPARATOR.len_utf8() * separators.partition_point(|&end| end < at || (include_at && end == at))
    };
    if block == Block::Code {
        spans.push(Span::new(0, words.len()).mono());
        spans.extend(colors);
    } else {
        for mark in edit.marks() {
            let (start, end) = (mark.start.max(range.start), mark.end.min(range.end));
            if start >= end {
                continue;
            }
            let original = start - range.start..end - range.start;
            let (start, end) = (remap(original.start, true), remap(original.end, false));
            let span = Span::new(start, end);
            let span = match &mark.mark {
                Mark::Bold => span.weight(650.0),
                Mark::Italic => span.italic(),
                Mark::Underline => span.underline(p.text),
                Mark::Code => span.mono().bg(p.surface, 2.0),
                Mark::Link(url) => {
                    links.push(Link::new(start..end, url.clone()));
                    span.color(p.accent).underline(p.accent.alpha(0.65))
                }
                Mark::Strike => {
                    strikes.push((start, end, None));
                    continue;
                }
                Mark::Pill(id) => {
                    let absolute = mark.start;
                    match pill(id, start..end, absolute, &words[original.clone()], look, p) {
                        Inline::Chip { style, link, span: chip_span, hides } => {
                            if hides {
                                hidden.push(original);
                            }
                            pills.push((start..end, style));
                            links.extend(link.map(|link| *link));
                            match chip_span {
                                Some(chip_span) => chip_span,
                                None => continue,
                            }
                        }
                        Inline::Plain => continue,
                    }
                }
                _ => continue,
            };
            spans.push(span);
        }
    }
    // Hidden words never reach the text: not the screen reader, not a copied selection. Blanks
    // of the same length keep every other range where it is.
    let mut shown = words.to_owned();
    for range in hidden {
        shown.replace_range(range.clone(), &" ".repeat(range.len()));
    }
    let accessible = (!separators.is_empty()).then(|| shown.clone());
    for &end in separators.iter().rev() {
        shown.insert(end, SEPARATOR);
        let at = remap(end, false);
        spans.push(Span::new(at, at + SEPARATOR.len_utf8()).size(0.1));
    }
    for (range, _, _) in &artwork {
        let (start, end) = (remap(range.start, true), remap(range.end, false));
        spans.push(Span::new(start, end).color(Color::TRANSPARENT).size(0.1));
    }
    let mut content = rune::rich(shown, spans).key(key.to_owned());
    if let Some(accessible) = accessible {
        content = content.label(accessible);
    }
    for end in separators.iter().copied() {
        let at = remap(end, false);
        content = content.uncopied(at..at + SEPARATOR.len_utf8());
    }
    let content = match fit {
        Fit::Wrap(style, color) => content
            .font(style)
            .color(color)
            .self_start()
            .shrink()
            .min_h(22.0)
            // A word longer than the line (a link, say) breaks rather than runs out of the message.
            .wrap_anywhere(),
        Fit::Line(style, color) => content.font(style).color(color).min_w(0.0).truncate(),
    };
    let mut content = content
        .strikes(strikes)
        .pills(pills)
        .selectable(true)
        .cursor(Cursor::Text)
        .on_press(|_: &mut State| {})
        .on_link(|s: &mut State, url, tx| s.open_link(url, tx));
    for (range, pixels, custom) in artwork {
        let size = match (custom, fit) {
            (true, _) => EMOJI,
            (false, Fit::Line(style, _)) => style.size,
            (false, _) => font::BODY.size + 2.0,
        };
        let radius = if custom { 3.0 } else { 0.0 };
        let icon = InlineIcon::image(pixels).size(size).gap(0.0).radius(radius);
        content = content.inline_icon(remap(range.start, true), icon);
    }
    if matches!(fit, Fit::Wrap(..)) {
        content = reach(content);
    }
    for link in links {
        content = content.link(link);
    }
    content
}

/// Artwork at an authored byte range. Rune's link hit areas and text decorations cover
/// glyphs, so linked, underlined and struck Unicode emoji keep their font rendering.
fn illustrations(
    edit: &TextEdit,
    line: &Range<usize>,
    look: Look<'_>,
    p: &Palette,
) -> Vec<(Range<usize>, rune::Pixels, bool)> {
    let mut occupied = Vec::new();
    let mut decorated = Vec::new();
    let mut custom = Vec::new();
    for mark in edit.marks() {
        let (start, end) = (mark.start.max(line.start), mark.end.min(line.end));
        if start >= end {
            continue;
        }
        let range = start - line.start..end - line.start;
        match &mark.mark {
            Mark::Pill(id) if id.starts_with("emote:") => custom.push((range, id)),
            Mark::Code | Mark::Pill(_) => occupied.push(range),
            Mark::Link(_) | Mark::Underline | Mark::Strike => decorated.push(range),
            _ => {}
        }
    }
    let intersects = |a: &Range<usize>, b: &Range<usize>| a.start < b.end && b.start < a.end;
    let available = |range: &Range<usize>| !occupied.iter().any(|chip| intersects(chip, range));
    let mut artwork: Vec<_> = custom
        .iter()
        .filter(|(range, _)| available(range))
        .filter_map(|(range, id)| {
            let media = matrix_core::MediaRef::from_mxc(id.strip_prefix("emote:")?).filter(|_| look.pictures)?;
            Some((range.clone(), look.media.image(&media)?.pixels().clone(), true))
        })
        .collect();
    let words = &edit.text[line.clone()];
    if !words.is_ascii() {
        artwork.extend(words.grapheme_indices(true).filter_map(|(start, sequence)| {
            let emoji = emojis::get(sequence).filter(|emoji| emoji.as_str() == sequence)?;
            let range = start..start + sequence.len();
            if !available(&range)
                || decorated.iter().any(|mark| intersects(mark, &range))
                || custom.iter().any(|(chip, _)| intersects(chip, &range))
            {
                return None;
            }
            let pixels = super::emoji_art::artwork(emoji, look.emoji_style, p.text)?;
            Some((range, pixels, false))
        }));
    }
    artwork.sort_unstable_by_key(|(range, _, _)| range.start);
    artwork
}

/// A short fingerprint of a spoiler's words, for its link id.
fn fingerprint(words: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    words.hash(&mut hasher);
    hasher.finish()
}

/// The side of a custom emoji in text.
const EMOJI: f32 = 22.0;

/// How a pill in a message draws.
enum Inline {
    /// A chip, maybe pressable, maybe with its words styled; `hides` its words (a spoiler not
    /// revealed yet).
    Chip { style: PillStyle, link: Option<Box<Link>>, span: Option<Span>, hides: bool },
    /// Its words as they are, with any artwork placed separately.
    Plain,
}

/// A message's pill (`matrix_core::html`): a mention opens its person or room and the reader's
/// own stands out; a spoiler hides its words until pressed; maths reads in italics on a
/// chip; a custom emoji shows its picture.
fn pill(id: &str, range: std::ops::Range<usize>, absolute: usize, words: &str, look: Look<'_>, p: &Palette) -> Inline {
    if let Some(link) = matrix_core::html::mention_link(id) {
        let mine = id.strip_prefix("user:") == Some(look.me);
        let style = if mine {
            PillStyle::new().bg(p.accent).fg(p.on_accent)
        } else {
            PillStyle::new().bg(p.accent.alpha(0.14)).fg(p.accent)
        };
        return Inline::Chip { style, link: Some(Box::new(Link::new(range, link))), span: None, hides: false };
    }
    if id == "spoiler" {
        // Revealed words stay revealed for these words only: an edit that hides others in their
        // place hides them again.
        let link_id = format!("{SPOILER_LINK}{}/{absolute}/{:x}", look.event_id, fingerprint(words));
        if look.revealed.contains(&link_id) {
            return Inline::Chip {
                style: PillStyle::new().bg(p.text_muted.alpha(0.15)).fg(p.text),
                link: None,
                span: None,
                hides: false,
            };
        }
        // The words take the chip's colour: there, but unread until pressed.
        let link = Link::new(range, link_id).label(crate::i18n::t!("message.spoiler"));
        return Inline::Chip {
            style: PillStyle::new().bg(p.text_muted).fg(p.text_muted),
            link: Some(Box::new(link)),
            span: None,
            hides: true,
        };
    }
    if id.starts_with("math") {
        let span = Span::new(range.start, range.end).italic();
        return Inline::Chip {
            style: PillStyle::new().bg(p.surface).fg(p.text),
            link: None,
            span: Some(span),
            hides: false,
        };
    }
    Inline::Plain
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn test_look(s: &State) -> Look<'_> {
        Look {
            event_id: "$m",
            me: "@me:x",
            revealed: &s.revealed,
            media: &s.media,
            pictures: true,
            emoji_style: s.settings.emoji_style,
        }
    }

    const EMOJI_LINE: &str = "Hi🙂👩‍💻!\u{200a}Good!";

    fn adjacent_emoji(s: &State, _: &mut rune::Cx<State>) -> El {
        let p = super::super::theme::palette(false);
        plain("adjacent", EMOJI_LINE, test_look(s), font::BODY, p.text, &p)
    }

    #[test]
    fn selecting_a_line_with_adjacent_emoji_should_keep_its_original_unicode() {
        let mut rt = Runtime::new(State::detached(), adjacent_emoji, (360.0, 120.0), Env::default());
        rt.frame(0.0);
        let line =
            rt.elements().into_iter().find(|element| element.key.as_deref() == Some("adjacent")).expect("message line");
        assert_eq!(line.label.as_deref(), Some(EMOJI_LINE));
        let rect = line.rect;
        let (x, y) = (rect[0] + rect[2] - REACH + 4.0, rect[1] + rect[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_move(rect[0], y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(rect[0], y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.selected_text().as_deref(), Some(EMOJI_LINE));
    }

    fn view(s: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html("Text 🙂<b>before <a href='https://matrix.org'>a link 🙂</a></b> after");
        formatted("message", &RichText { edit, code: Vec::new() }, test_look(s), &super::super::theme::palette(true))
    }

    #[test]
    fn hovering_a_message_link_should_show_the_pointer() {
        let mut rt = Runtime::new(State::detached(), view, (500.0, 100.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let (x, y) = rt.find("https://matrix.org").expect("the link");
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), Cursor::Pointer);
    }

    #[test]
    fn hovering_text_beside_a_link_should_keep_the_text_cursor() {
        let mut rt = Runtime::new(State::detached(), view, (500.0, 100.0), Env::new(true, 1.0));
        rt.frame(0.0);
        rt.pointer_move(5.0, 10.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), Cursor::Text);
    }

    fn hidden_spoiler(s: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html("<p>the end: <span data-pill=\"spoiler\">the butler did it</span></p>");
        formatted("message", &RichText { edit, code: Vec::new() }, test_look(s), &super::super::theme::palette(true))
    }

    #[test]
    fn a_hidden_spoiler_should_give_its_words_to_nothing() {
        let mut rt = Runtime::new(State::detached(), hidden_spoiler, (500.0, 100.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|element| element.text).collect();
        assert!(texts.iter().any(|text| text.starts_with("the end:")), "{texts:?}");
        assert!(texts.iter().all(|text| !text.contains("butler")), "{texts:?}");
    }

    fn multiline_lists(s: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html(
            "<ul><li>first<br>continuation<br>last line</li><li>second</li></ul><ol><li>numbered<br>continued</li><li>next</li></ol>",
        );
        formatted("message", &RichText { edit, code: Vec::new() }, test_look(s), &super::super::theme::palette(true))
    }

    #[test]
    fn soft_breaks_should_continue_the_same_list_item() {
        let mut rt = Runtime::new(State::detached(), multiline_lists, (500.0, 300.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let elements = rt.elements();
        let words: Vec<&str> = elements.iter().filter_map(|element| element.text.as_deref()).collect();
        assert!(
            words.contains(&"first\ncontinuation\nlast line"),
            "a continuation must share its paragraph: {words:?}"
        );
        assert_eq!(words.iter().filter(|word| **word == "•").count(), 2);
        assert!(words.contains(&"1.") && words.contains(&"2.") && !words.contains(&"3."));
    }

    fn long_blocks(s: &State, _: &mut rune::Cx<State>) -> El {
        let words = "word ".repeat(60);
        let edit = TextEdit::from_html(&format!(
            "<ul><li>{words}</li></ul><ol><li>{words}</li></ol><blockquote>{words}</blockquote><pre><code>{words}</code></pre>"
        ));
        formatted("message", &RichText { edit, code: Vec::new() }, test_look(s), &super::super::theme::palette(true))
    }

    #[test]
    fn long_lists_quotes_and_code_should_wrap_within_the_message() {
        let mut rt = Runtime::new(State::detached(), long_blocks, (300.0, 1100.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let blocks: Vec<_> =
            rt.elements().into_iter().filter(|e| e.text.as_ref().is_some_and(|text| text.len() > 50)).collect();
        assert_eq!(blocks.len(), 4);
        for block in blocks {
            // The box reaches past the words; the words stay within the message.
            assert!(block.rect[0] + block.rect[2] - REACH <= 300.5, "overflow: {:?}", block.rect);
            assert!(block.rect[3] > 22.0, "the block must wrap");
        }
    }

    #[test]
    fn code_in_a_known_language_should_take_its_colours_on_every_line() {
        let rich = RichText::from_html("<pre><code class=\"language-rust\">/* one\ntwo */\nfn main() {}</code></pre>");
        let colors = code_colors(&rich, &place_code(&rich.edit, &rich.code), &super::super::theme::palette(true));
        let lines: Vec<_> = rich.edit.paragraphs().into_iter().map(|line| line.range.start).collect();
        // The comment's second line is still a comment: the block is highlighted whole.
        let comment = &colors[&lines[1]];
        assert_eq!(comment.first().map(|span| (span.start, span.end)), Some((0, 6)), "{comment:?}");
        assert!(!colors[&lines[2]].is_empty());
    }

    #[test]
    fn code_blocks_that_follow_each_other_should_each_take_their_language() {
        let rich = RichText::from_html(
            "<pre><code class=\"language-rust\">fn main() {}</code></pre><pre><code>fn main() {}</code></pre>",
        );
        let colors = code_colors(&rich, &place_code(&rich.edit, &rich.code), &super::super::theme::palette(true));
        let lines: Vec<_> = rich.edit.paragraphs().into_iter().map(|line| line.range.start).collect();
        assert!(!colors[&lines[0]].is_empty());
        assert!(!colors.contains_key(&lines[1]), "the second block has no language");
    }

    fn two_blocks(s: &State, _: &mut rune::Cx<State>) -> El {
        let rich = RichText::from_html("<pre><code>a\n</code></pre>\n<pre><code>b\n</code></pre>");
        formatted("message", &rich, test_look(s), &super::super::theme::palette(true))
    }

    #[test]
    fn a_blank_line_between_code_blocks_should_get_no_copy_button() {
        let mut rt = Runtime::new(State::detached(), two_blocks, (500.0, 300.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let buttons = rt.elements().into_iter().filter_map(|e| e.key).filter(|key| key.contains("-copy-code-")).count();
        assert_eq!(buttons, 2);
    }

    fn one_line_block(s: &State, _: &mut rune::Cx<State>) -> El {
        let rich = RichText::from_html("<pre><code>ls</code></pre><p>after</p>");
        formatted("message", &rich, test_look(s), &super::super::theme::palette(true))
    }

    #[test]
    fn a_one_line_code_blocks_copy_button_should_stay_within_it() {
        let mut rt = Runtime::new(State::detached(), one_line_block, (500.0, 300.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let rect = |rt: &Runtime<State>, part: &str| {
            let key = rt.elements().into_iter().filter_map(|e| e.key).find(|key| key.contains(part)).expect(part);
            rt.presented(&key).expect(part).0
        };
        let (block, button) = (rect(&rt, "-code-0"), rect(&rt, "-copy-code-"));
        assert!(button[1] + button[3] <= block[1] + block[3], "button {button:?}, block {block:?}");
    }

    #[test]
    fn code_without_a_language_should_stay_plain() {
        let rich = RichText::from_html("<pre><code>fn main() {}</code></pre>");
        assert!(
            code_colors(&rich, &place_code(&rich.edit, &rich.code), &super::super::theme::palette(true)).is_empty()
        );
    }
}
