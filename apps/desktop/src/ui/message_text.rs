//! Read-only message formatting. Selectable text keeps link hit testing and text selection
//! without making a received message a text field.

use std::collections::HashSet;

use rune::{Block, Color, Cursor, El, InlineIcon, Link, Mark, PillStyle, Span, TextEdit, col, row, text};

use super::theme::{Palette, font};
use crate::state::{MediaCache, SPOILER_LINK, State};

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
}

/// How far beside a message's text a press still lands on the text: a click or drag that
/// starts just left or right of the words selects them rather than opening the thread. Less
/// than the gap to the sender's picture and the readers' column, which keep their own presses.
pub(super) const REACH: f32 = 8.0;

/// Widens a message text's press target by [`REACH`] on each side without moving its words.
pub fn reach(text: El) -> El {
    text.px(REACH).mx(-REACH)
}

pub fn formatted(key: &str, edit: &TextEdit, look: Look<'_>, p: &Palette) -> El {
    let mut body = col().key(key.to_owned()).w_full().selection_group(true);
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
        let content = inline(&line_key, edit, range, block, look, p);
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
            Block::Code => row().w_full().px(8.0).fill(p.surface).child(content),
            Block::Paragraph => {
                numbers.fill(0);
                content
            }
        };
        body = body.child(paragraph);
    }
    body
}

fn inline(key: &str, edit: &TextEdit, range: std::ops::Range<usize>, block: Block, look: Look<'_>, p: &Palette) -> El {
    let words = &edit.text[range.clone()];
    let mut spans = Vec::new();
    let mut links = Vec::new();
    let mut strikes = Vec::new();
    let mut pills = Vec::new();
    let mut emoji = Vec::new();
    if block == Block::Code {
        spans.push(Span::new(0, words.len()).mono());
    } else {
        for mark in edit.marks() {
            let (start, end) = (mark.start.max(range.start), mark.end.min(range.end));
            if start >= end {
                continue;
            }
            let (start, end) = (start - range.start, end - range.start);
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
                    match pill(id, start..end, absolute, &words[start..end], look, p) {
                        Inline::Chip { style, link, span: chip_span } => {
                            pills.push((start..end, style));
                            links.extend(link.map(|link| *link));
                            match chip_span {
                                Some(chip_span) => chip_span,
                                None => continue,
                            }
                        }
                        Inline::Emoji(pixels) => {
                            emoji.push((start, pixels));
                            // The short code stays the text (copying it gives `:blobcat:`), unseen.
                            span.color(Color::TRANSPARENT).size(1.0)
                        }
                        Inline::Plain => continue,
                    }
                }
                _ => continue,
            };
            spans.push(span);
        }
    }
    let mut content = rune::rich(words.to_owned(), spans)
        .key(key.to_owned())
        .font(font::BODY)
        .color(p.text)
        .self_start()
        .shrink()
        .min_h(22.0)
        // A word longer than the line (a link, say) breaks rather than runs out of the message.
        .wrap_anywhere()
        .strikes(strikes)
        .pills(pills)
        .selectable(true)
        .cursor(Cursor::Text)
        .on_press(|_: &mut State| {})
        .on_link(|s: &mut State, url, tx| s.open_link(url, tx));
    for (at, pixels) in emoji {
        content = content.inline_icon(at, InlineIcon::image(pixels).size(EMOJI).gap(0.0).radius(3.0));
    }
    content = reach(content);
    for link in links {
        content = content.link(link);
    }
    content
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
    /// A chip, maybe pressable, maybe with its words styled.
    Chip { style: PillStyle, link: Option<Box<Link>>, span: Option<Span> },
    /// A custom emoji's picture.
    Emoji(rune::Pixels),
    /// Its words as they are (a custom emoji whose picture isn't here yet).
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
        return Inline::Chip { style, link: Some(Box::new(Link::new(range, link))), span: None };
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
            };
        }
        // The words take the chip's colour: there, but unread until pressed.
        let link = Link::new(range, link_id).label(crate::i18n::t!("message.spoiler"));
        return Inline::Chip {
            style: PillStyle::new().bg(p.text_muted).fg(p.text_muted),
            link: Some(Box::new(link)),
            span: None,
        };
    }
    if id.starts_with("math") {
        let span = Span::new(range.start, range.end).italic();
        return Inline::Chip { style: PillStyle::new().bg(p.surface).fg(p.text), link: None, span: Some(span) };
    }
    if let Some(url) = id.strip_prefix("emote:") {
        let pixels = matrix_core::MediaRef::from_mxc(url)
            .and_then(|media| look.media.image(&media).map(|image| image.pixels().clone()));
        return pixels.map_or(Inline::Plain, Inline::Emoji);
    }
    Inline::Plain
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn test_look(s: &State) -> Look<'_> {
        Look { event_id: "$m", me: "@me:x", revealed: &s.revealed, media: &s.media }
    }

    fn view(s: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html("Text <b>before <a href='https://matrix.org'>a link</a></b> after");
        formatted("message", &edit, test_look(s), &super::super::theme::palette(true))
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

    fn multiline_lists(s: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html(
            "<ul><li>first<br>continuation<br>last line</li><li>second</li></ul><ol><li>numbered<br>continued</li><li>next</li></ol>",
        );
        formatted("message", &edit, test_look(s), &super::super::theme::palette(true))
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
        formatted("message", &edit, test_look(s), &super::super::theme::palette(true))
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
}
