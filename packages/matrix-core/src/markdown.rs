//! Markdown in plain text, as Matrix HTML: room topics, and message bodies their sender sent
//! without formatting (bots and bridges often write Markdown there).
//!
//! CommonMark with strikethrough. Raw HTML in the text shows as typed, never as markup, and
//! every line break stays one: chat text isn't wrapped by hand the way Markdown files are. Text
//! without Markdown formatting gives `None`, so callers keep it exactly as written.

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

/// `text`'s Markdown as Matrix HTML, or `None` when it uses none (plain paragraphs only).
///
/// ```
/// use matrix_core::markdown::to_html;
/// assert_eq!(to_html("**ship** it").as_deref(), Some("<p><strong>ship</strong> it</p>"));
/// assert_eq!(to_html("just words"), None);
/// ```
pub fn to_html(text: &str) -> Option<String> {
    let mut html = String::new();
    let mut formatted = false;
    for event in Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Start(tag) => {
                formatted |= !matches!(tag, Tag::Paragraph | Tag::HtmlBlock);
                open(&tag, &mut html);
            }
            Event::End(tag) => close(tag, &mut html),
            Event::Code(code) => {
                formatted = true;
                html.push_str("<code>");
                escape(&code, &mut html);
                html.push_str("</code>");
            }
            Event::Text(words) | Event::Html(words) | Event::InlineHtml(words) => escape(&words, &mut html),
            Event::SoftBreak | Event::HardBreak => html.push_str("<br>"),
            Event::Rule => {
                formatted = true;
                html.push_str("<hr>");
            }
            Event::InlineMath(words) | Event::DisplayMath(words) | Event::FootnoteReference(words) => {
                escape(&words, &mut html);
            }
            Event::TaskListMarker(_) => {}
        }
    }
    formatted.then_some(html)
}

fn open(tag: &Tag<'_>, html: &mut String) {
    match tag {
        Tag::Paragraph => html.push_str("<p>"),
        Tag::Heading { level, .. } => html.push_str(&format!("<{level}>")),
        Tag::BlockQuote(_) => html.push_str("<blockquote>"),
        Tag::CodeBlock(kind) => {
            html.push_str("<pre><code");
            if let CodeBlockKind::Fenced(language) = kind
                && let Some(language) = language.split_whitespace().next()
            {
                html.push_str(" class=\"language-");
                escape(language, html);
                html.push('"');
            }
            html.push('>');
        }
        Tag::List(Some(1)) => html.push_str("<ol>"),
        Tag::List(Some(start)) => html.push_str(&format!("<ol start=\"{start}\">")),
        Tag::List(None) => html.push_str("<ul>"),
        Tag::Item => html.push_str("<li>"),
        Tag::Emphasis => html.push_str("<em>"),
        Tag::Strong => html.push_str("<strong>"),
        Tag::Strikethrough => html.push_str("<del>"),
        // An image shows as a link to it, its words the description.
        Tag::Link { link_type, dest_url, .. } | Tag::Image { link_type, dest_url, .. } => {
            html.push_str("<a href=\"");
            if matches!(link_type, LinkType::Email) {
                html.push_str("mailto:");
            }
            escape(dest_url, html);
            html.push_str("\">");
        }
        _ => {}
    }
}

fn close(tag: TagEnd, html: &mut String) {
    match tag {
        TagEnd::Paragraph => html.push_str("</p>"),
        TagEnd::Heading(level) => html.push_str(&format!("</{level}>")),
        TagEnd::BlockQuote(_) => html.push_str("</blockquote>"),
        TagEnd::CodeBlock => {
            // The block's last line ends in a line break, which isn't another line.
            if html.ends_with('\n') {
                html.pop();
            }
            html.push_str("</code></pre>");
        }
        TagEnd::List(true) => html.push_str("</ol>"),
        TagEnd::List(false) => html.push_str("</ul>"),
        TagEnd::Item => html.push_str("</li>"),
        TagEnd::Emphasis => html.push_str("</em>"),
        TagEnd::Strong => html.push_str("</strong>"),
        TagEnd::Strikethrough => html.push_str("</del>"),
        TagEnd::Link | TagEnd::Image => html.push_str("</a>"),
        _ => {}
    }
}

fn escape(text: &str, html: &mut String) {
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            '\'' => html.push_str("&#39;"),
            c => html.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::to_html;

    #[test]
    fn plain_paragraphs_should_stay_plain() {
        assert_eq!(to_html("Hello\n\nSee you at 5 * 3 = 15"), None);
    }

    #[test]
    fn words_inside_a_word_should_not_turn_italic() {
        assert_eq!(to_html("call snake_case_names"), None);
    }

    #[test]
    fn links_should_become_anchors() {
        assert_eq!(
            to_html("[Github](https://github.com/x) | [Office](https://bbb.example/join)").as_deref(),
            Some(
                "<p><a href=\"https://github.com/x\">Github</a> | <a href=\"https://bbb.example/join\">Office</a></p>"
            )
        );
    }

    #[test]
    fn line_breaks_should_stay_line_breaks() {
        assert_eq!(to_html("**one**\ntwo").as_deref(), Some("<p><strong>one</strong><br>two</p>"));
    }

    #[test]
    fn raw_html_should_show_as_typed() {
        assert_eq!(
            to_html("*hi* <img src=x onerror=alert(1)>").as_deref(),
            Some("<p><em>hi</em> &lt;img src=x onerror=alert(1)&gt;</p>")
        );
    }

    #[test]
    fn inline_code_should_keep_its_markdown_literal() {
        assert_eq!(to_html("run `**not bold**`").as_deref(), Some("<p>run <code>**not bold**</code></p>"));
    }

    #[test]
    fn a_fenced_block_should_become_code_without_its_last_line_break() {
        assert_eq!(
            to_html("```rust\nfn main() {}\n```").as_deref(),
            Some("<pre><code class=\"language-rust\">fn main() {}</code></pre>")
        );
    }

    #[test]
    fn a_heading_should_keep_its_level() {
        assert_eq!(to_html("## Notes").as_deref(), Some("<h2>Notes</h2>"));
    }

    #[test]
    fn lists_should_keep_their_items() {
        assert_eq!(to_html("- one\n- two").as_deref(), Some("<ul><li>one</li><li>two</li></ul>"));
    }

    #[test]
    fn strikethrough_should_become_deleted_text() {
        assert_eq!(to_html("~~old~~ new").as_deref(), Some("<p><del>old</del> new</p>"));
    }

    #[test]
    fn an_image_should_become_a_link_to_it() {
        assert_eq!(
            to_html("![logo](https://x.org/logo.png)").as_deref(),
            Some("<p><a href=\"https://x.org/logo.png\">logo</a></p>")
        );
    }

    #[test]
    fn an_email_autolink_should_mail_its_address() {
        assert_eq!(to_html("<ada@x.org>").as_deref(), Some("<p><a href=\"mailto:ada@x.org\">ada@x.org</a></p>"));
    }
}
