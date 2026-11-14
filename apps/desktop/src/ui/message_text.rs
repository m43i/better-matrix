//! Read-only message formatting. Selectable text keeps link hit testing and text selection
//! without making a received message a text field.

use rune::{Block, Cursor, El, Link, Mark, Span, TextEdit, col, row, text};

use super::theme::{Palette, font};
use crate::state::State;

pub fn formatted(key: &str, edit: &TextEdit, p: &Palette) -> El {
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
        let content = inline(&line_key, edit, range, block, p);
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

fn inline(key: &str, edit: &TextEdit, range: std::ops::Range<usize>, block: Block, p: &Palette) -> El {
    let words = &edit.text[range.clone()];
    let mut spans = Vec::new();
    let mut links = Vec::new();
    let mut strikes = Vec::new();
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
                _ => continue,
            };
            spans.push(span);
        }
    }
    let mut content = rune::rich(words.to_owned(), spans)
        .key(key.to_owned())
        .font(font::BODY)
        .color(p.text)
        .w_full()
        .min_w(0.0)
        .grow(1.0)
        .min_h(22.0)
        .wrap()
        .strikes(strikes)
        .selectable(true)
        .cursor(Cursor::Text)
        .on_press(|_: &mut State| {})
        .on_link(|s: &mut State, url, tx| s.open_link(url, tx));
    for link in links {
        content = content.link(link);
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn view(_: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html("Text <b>before <a href='https://matrix.org'>a link</a></b> after");
        formatted("message", &edit, &super::super::theme::palette(true))
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

    fn multiline_lists(_: &State, _: &mut rune::Cx<State>) -> El {
        let edit = TextEdit::from_html(
            "<ul><li>first<br>continuation<br>last line</li><li>second</li></ul><ol><li>numbered<br>continued</li><li>next</li></ol>",
        );
        formatted("message", &edit, &super::super::theme::palette(true))
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
}
