//! Text emoticons (`:)`, `:D`, `<3`) and the emoji a message is sent with in their place. The
//! composer keeps what was typed; the swap happens on sending.
//!
//! An emoticon counts only as a word of its own: whitespace (or the start) before it, and
//! whitespace, the end or closing punctuation (`.`, `,`, `!`, `?`) after it. So `2<3`,
//! `f(:)`, `https://`, `:))` and `o.Obj` stay as they are, and so does anything between
//! backticks. `<3` beside a number (`2 <3 4`) reads as maths and stays too.

use std::ops::Range;

/// Each emoticon and its emoji.
const EMOTICONS: &[(&str, &str)] = &[
    (":)", "🙂"),
    (":-)", "🙂"),
    (":D", "😃"),
    (":-D", "😃"),
    (":P", "😛"),
    (":p", "😛"),
    (":-P", "😛"),
    (";)", "😉"),
    (";-)", "😉"),
    (":(", "🙁"),
    (":-(", "🙁"),
    (":/", "😕"),
    (":-/", "😕"),
    (":O", "😮"),
    (":o", "😮"),
    ("o.O", "🤨"),
    ("O.o", "🤨"),
    ("<3", "❤️"),
    ("</3", "💔"),
];

/// Where `text` holds emoticons, in order, each with its emoji.
///
/// ```
/// let found = matrix_core::emoticons::find("nice :) but 2<3");
/// assert_eq!(found, [(5..7, "🙂")]);
/// ```
pub fn find(text: &str) -> Vec<(Range<usize>, &'static str)> {
    let code = code_spans(text);
    let mut found = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let before = text[..at].chars().next_back();
        let starts_word = before.is_none_or(char::is_whitespace);
        let matched = starts_word
            .then(|| {
                EMOTICONS.iter().find(|(emoticon, _)| {
                    let end = at + emoticon.len();
                    text[at..].starts_with(emoticon)
                        && text[end..].chars().next().is_none_or(ends_word)
                        && !code.iter().any(|span| span.start < end && at < span.end)
                        && !(*emoticon == "<3" && beside_number(text, at, end))
                })
            })
            .flatten();
        match matched {
            Some((emoticon, emoji)) => {
                found.push((at..at + emoticon.len(), *emoji));
                at += emoticon.len();
            }
            None => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    found
}

/// `text` with its emoticons as emoji.
///
/// ```
/// assert_eq!(matrix_core::emoticons::replace("see you <3"), "see you ❤️");
/// ```
pub fn replace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for (range, emoji) in find(text) {
        out.push_str(&text[from..range.start]);
        out.push_str(emoji);
        from = range.end;
    }
    out.push_str(&text[from..]);
    out
}

fn ends_word(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?')
}

/// Whether the words on either side of `at..end` are numbers: `2 <3 4` compares.
fn beside_number(text: &str, at: usize, end: usize) -> bool {
    let before = text[..at].trim_end().chars().next_back();
    let after = text[end..].trim_start().chars().next();
    before.is_some_and(|c| c.is_ascii_digit()) || after.is_some_and(|c| c.is_ascii_digit())
}

/// The spans between matching runs of backticks (Markdown code), backticks included: a run
/// opens one and the next run as long closes it.
fn code_spans(text: &str) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    for (at, _) in text.match_indices('`') {
        match runs.last_mut() {
            Some(run) if run.end == at => run.end += 1,
            _ => runs.push(at..at + 1),
        }
    }
    let mut spans = Vec::new();
    let mut next = 0;
    while let Some(open) = runs.get(next) {
        match runs[next + 1..].iter().position(|run| run.len() == open.len()) {
            Some(offset) => {
                spans.push(open.start..runs[next + 1 + offset].end);
                next += offset + 2;
            }
            None => next += 1,
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::replace;

    #[test]
    fn every_listed_emoticon_should_become_its_emoji() {
        assert_eq!(replace(":) :P :D :( :/ o.O O.o <3"), "🙂 😛 😃 🙁 😕 🤨 🤨 ❤️");
    }

    #[test]
    fn an_emoticon_at_the_end_of_a_sentence_should_be_replaced() {
        assert_eq!(replace("thanks :)."), "thanks 🙂.");
        assert_eq!(replace("ok :D!"), "ok 😃!");
    }

    #[test]
    fn an_emoticon_on_its_own_line_should_be_replaced() {
        assert_eq!(replace("hi\n:)\nbye"), "hi\n🙂\nbye");
    }

    #[test]
    fn a_comparison_without_spaces_should_stay() {
        assert_eq!(replace("2<3"), "2<3");
        assert_eq!(replace("if a<3 then"), "if a<3 then");
    }

    #[test]
    fn a_comparison_beside_a_number_should_stay() {
        assert_eq!(replace("2 <3"), "2 <3");
        assert_eq!(replace("x <3 4"), "x <3 4");
    }

    #[test]
    fn braces_and_brackets_should_stay() {
        assert_eq!(replace("f(:) (:P) [:D] {:(}"), "f(:) (:P) [:D] {:(}");
        assert_eq!(replace("map(|x| x:)"), "map(|x| x:)");
    }

    #[test]
    fn a_doubled_emoticon_should_stay() {
        assert_eq!(replace("yay :))"), "yay :))");
    }

    #[test]
    fn web_addresses_and_paths_should_stay() {
        assert_eq!(replace("see https://example.org/:/x and C:/Users"), "see https://example.org/:/x and C:/Users");
        assert_eq!(replace("run :/usr/bin"), "run :/usr/bin");
    }

    #[test]
    fn words_holding_an_emoticon_should_stay() {
        assert_eq!(replace("foo.Obj o.Of re:o x:D"), "foo.Obj o.Of re:o x:D");
    }

    #[test]
    fn times_and_ratios_should_stay() {
        assert_eq!(replace("at 12:00 or 3:2"), "at 12:00 or 3:2");
    }

    #[test]
    fn inline_code_should_stay() {
        assert_eq!(replace("type `:)` for :)"), "type `:)` for 🙂");
    }

    #[test]
    fn a_code_block_should_stay() {
        assert_eq!(replace("```\nprint(':)')\n:)\n```\n:)"), "```\nprint(':)')\n:)\n```\n🙂");
    }

    #[test]
    fn an_unclosed_backtick_should_not_hide_what_follows() {
        assert_eq!(replace("a ` b :)"), "a ` b 🙂");
    }

    #[test]
    fn text_without_emoticons_should_stay_as_written() {
        assert_eq!(replace("plain words, nothing else"), "plain words, nothing else");
    }

    #[test]
    fn a_broken_heart_should_be_replaced() {
        assert_eq!(replace("sad </3"), "sad 💔");
    }
}
