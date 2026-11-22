//! Matrix HTML's inline objects, and the pills apps edit and draw them as.
//!
//! A message's HTML carries things that aren't formatting: mentions (links to a person or room
//! on matrix.to, or a `matrix:` URI), spoilers (`<span data-mx-spoiler>`), maths
//! (`data-mx-maths`, inline on a `<span>`, a block on a `<div>`) and custom emoji
//! (`<img data-mx-emoticon>`). A text field holds each as one pill
//! (`<span data-pill="id">label</span>`, as Rune's `TextEdit` reads and writes its pills), so
//! the caret steps over it and a press can act on it. [`to_pills`] turns Matrix HTML into
//! pills, for showing a message or editing one; [`from_pills`] turns a field's pills back into
//! Matrix HTML to send. Pill ids:
//!
//! - `user:@ada:example.org`, `room:#rust:matrix.org` (or a room ID): a mention.
//! - `everyone`: `@room`, which goes as plain text.
//! - `emote:mxc://…`: a custom emoji; the label is its short code (`:blobcat:`).
//! - `spoiler`: hidden text, the label.
//! - `math`: a formula being written, the label its LaTeX; shown, `math:<latex>` (or
//!   `math-block:<latex>`) with the formula in Unicode as the label ([`crate::math`]).

use crate::permalinks::{LinkTarget, Permalink};

/// The height custom emoji are sent at, in pixels, as other clients send them.
const EMOTE_HEIGHT: u32 = 32;

/// What pills are for: drawing a message, or editing one in a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pills {
    /// A message shown: maths as Unicode text.
    Display,
    /// A message in a field: maths as the LaTeX being written.
    Compose,
}

/// A mention pill's id for a user or room ID or alias; `None` for anything else.
///
/// ```
/// assert_eq!(matrix_core::html::mention_id("@ada:x.org").as_deref(), Some("user:@ada:x.org"));
/// assert_eq!(matrix_core::html::mention_id("#rust:x.org").as_deref(), Some("room:#rust:x.org"));
/// ```
pub fn mention_id(id: &str) -> Option<String> {
    match id.chars().next()? {
        '@' => Some(format!("user:{id}")),
        '#' | '!' => Some(format!("room:{id}")),
        _ => None,
    }
}

/// The matrix.to link a mention pill's id stands for.
///
/// ```
/// assert_eq!(matrix_core::html::mention_link("user:@ada:x.org").as_deref(), Some("https://matrix.to/#/@ada:x.org"));
/// ```
pub fn mention_link(pill: &str) -> Option<String> {
    let id = pill.strip_prefix("user:").or_else(|| pill.strip_prefix("room:"))?;
    // One segment of the fragment: a `/` in an ID (user IDs may hold one) would start another.
    let encoded: String = id
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'@' | b'#' | b'!' | b':' => {
                char::from(byte).to_string()
            }
            byte => format!("%{byte:02X}"),
        })
        .collect();
    Some(format!("https://matrix.to/#/{encoded}"))
}

/// Matrix HTML with its mentions, spoilers, maths and custom emoji as pills. A link to a
/// message stays a link (it opens the message), as does a link whose words aren't the
/// mention's (a sender's "click here").
///
/// ```
/// use matrix_core::html::{to_pills, Pills};
/// assert_eq!(
///     to_pills("hi <a href=\"https://matrix.to/#/@ada:x.org\">Ada</a>", Pills::Display),
///     "hi <span data-pill=\"user:@ada:x.org\">Ada</span>"
/// );
/// ```
pub fn to_pills(html: &str, purpose: Pills) -> String {
    let mut out = String::with_capacity(html.len());
    // What each open element's closing tag becomes (`None`: as it is).
    let mut open: Vec<(String, Option<&'static str>)> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>').map(|end| start + end) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let tag = &rest[start..=end];
        rest = &rest[end + 1..];
        let Some((name, closing)) = tag_name(tag) else {
            out.push_str(tag);
            continue;
        };
        if closing {
            match open.iter().rposition(|(open, _)| *open == name) {
                Some(index) => {
                    let (_, replacement) = open.remove(index);
                    out.push_str(replacement.unwrap_or(tag));
                }
                None => out.push_str(tag),
            }
            continue;
        }
        match name.as_str() {
            // A link around a spoiler isn't one: its words would show as the link's. Nor is a code
            // block around one a code block: a field keeps no pills in code blocks, so its words
            // would show as code.
            "a" | "pre" if inner_html(rest, &name).contains("data-mx-spoiler") => open.push((name, Some(""))),
            "a" => {
                let pill = attribute(tag, "href").and_then(|href| link_pill(&href));
                match pill {
                    Some(id) => {
                        out.push_str(&pill_open(&id));
                        open.push((name, Some("</span>")));
                    }
                    None => {
                        out.push_str(tag);
                        open.push((name, None));
                    }
                }
            }
            // Matrix HTML's only images are the homeserver's: custom emoji. The SDK's sanitiser
            // drops `data-mx-emoticon` (not in the spec's attribute list), so an `mxc://` source
            // is what marks one.
            "img" => match attribute(tag, "src") {
                Some(src) if src.starts_with("mxc://") => {
                    let label =
                        attribute(tag, "alt").or_else(|| attribute(tag, "title")).unwrap_or_else(|| ":emoji:".into());
                    out.push_str(&pill_open(&format!("emote:{src}")));
                    out.push_str(&escape(&label));
                    out.push_str("</span>");
                }
                // Other images can't show inline: their description does.
                _ => out.push_str(&escape(&attribute(tag, "alt").unwrap_or_default())),
            },
            "span" | "div" => {
                // A spoiler comes first: whatever else the element says, its words stay hidden.
                let spoiler = attribute(tag, "data-mx-spoiler").is_some();
                if let Some(latex) = attribute(tag, "data-mx-maths").filter(|_| !spoiler) {
                    let block = name == "div";
                    rest = element_text(rest, &name).1;
                    let (id, label) = match purpose {
                        Pills::Display => (
                            format!("{}:{latex}", if block { "math-block" } else { "math" }),
                            crate::math::to_unicode(&latex),
                        ),
                        Pills::Compose => ("math".to_owned(), latex.clone()),
                    };
                    if block {
                        out.push_str("<p>");
                    }
                    out.push_str(&pill_open(&id));
                    out.push_str(&escape(&label));
                    out.push_str("</span>");
                    if block {
                        out.push_str("</p>");
                    }
                } else if spoiler {
                    // Hidden words are words only: nothing inside (a mention, a custom emoji, a
                    // link) may show or act through them.
                    let (words, after) = element_text(rest, &name);
                    rest = after;
                    out.push_str(&pill_open("spoiler"));
                    out.push_str(&escape(&words));
                    out.push_str("</span>");
                } else {
                    out.push_str(tag);
                    if !tag.ends_with("/>") {
                        open.push((name, None));
                    }
                }
            }
            _ => {
                out.push_str(tag);
                if !is_void(&name) && !tag.ends_with("/>") {
                    open.push((name, None));
                }
            }
        }
    }
    out.push_str(rest);
    out
}

/// A field's HTML with its pills as Matrix HTML, to send.
///
/// ```
/// assert_eq!(
///     matrix_core::html::from_pills("<p>hi <span data-pill=\"user:@ada:x.org\">Ada</span></p>"),
///     "<p>hi <a href=\"https://matrix.to/#/@ada:x.org\">Ada</a></p>"
/// );
/// ```
pub fn from_pills(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut open: Vec<Option<&'static str>> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>').map(|end| start + end) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let tag = &rest[start..=end];
        rest = &rest[end + 1..];
        match tag_name(tag) {
            Some((name, false)) if name == "span" => match attribute(tag, "data-pill") {
                Some(id) => {
                    if let Some(link) = mention_link(&id) {
                        out.push_str(&format!("<a href=\"{}\">", escape(&link)));
                        open.push(Some("</a>"));
                    } else if id == "spoiler" {
                        out.push_str("<span data-mx-spoiler>");
                        open.push(Some("</span>"));
                    } else {
                        // The rest are their label's text alone.
                        let (inner, after) = element_text(rest, "span");
                        rest = after;
                        out.push_str(&atom_html(&id, &inner));
                    }
                }
                None => {
                    out.push_str(tag);
                    open.push(None);
                }
            },
            Some((name, true)) if name == "span" => out.push_str(open.pop().flatten().unwrap_or(tag)),
            _ => out.push_str(tag),
        }
    }
    out.push_str(rest);
    out
}

/// `words` hidden as one spoiler, line breaks kept: the Matrix HTML `/spoiler` sends.
///
/// ```
/// assert_eq!(matrix_core::html::spoiler("a <b>\nc"), "<span data-mx-spoiler>a &lt;b&gt;<br />c</span>");
/// ```
pub fn spoiler(words: &str) -> String {
    format!("<span data-mx-spoiler>{}</span>", escape(words).replace('\n', "<br />"))
}

/// Whether formatted text hides a spoiler. Its plain fallback may then say what the spoiler
/// hides, so nothing is read from it (link previews, GIFs) before the spoiler is shown.
///
/// ```
/// assert!(matrix_core::html::holds_spoiler("see <span data-mx-spoiler>https://x.org</span>"));
/// assert!(!matrix_core::html::holds_spoiler("see <b>https://x.org</b>"));
/// ```
pub fn holds_spoiler(html: &str) -> bool {
    html.contains("data-mx-spoiler")
}

/// The Matrix HTML for a pill that is its label's text alone: custom emoji, `@room`, maths.
fn atom_html(id: &str, label: &str) -> String {
    if let Some(src) = id.strip_prefix("emote:") {
        let label = escape(label);
        return format!(
            "<img data-mx-emoticon src=\"{}\" alt=\"{label}\" title=\"{label}\" height=\"{EMOTE_HEIGHT}\" />",
            escape(src)
        );
    }
    if id == "math" || id.starts_with("math:") {
        let latex = id.strip_prefix("math:").unwrap_or(label);
        return format!("<span data-mx-maths=\"{}\"><code>{}</code></span>", escape(latex), escape(latex));
    }
    escape(label)
}

/// The pill a link stands for: a person or room it names, not a message in one.
fn link_pill(href: &str) -> Option<String> {
    let link = Permalink::parse(href)?;
    match link.target {
        LinkTarget::User(user) => mention_id(&user),
        LinkTarget::Room { room, event: None } => mention_id(&room),
        LinkTarget::Room { event: Some(_), .. } => None,
    }
}

fn pill_open(id: &str) -> String {
    format!("<span data-pill=\"{}\">", escape(id))
}

/// A tag's lower-case name and whether it closes; `None` for comments and doctypes.
fn tag_name(tag: &str) -> Option<(String, bool)> {
    let inner = tag.strip_prefix('<')?.strip_suffix('>')?;
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(inner) => (true, inner),
        None => (false, inner),
    };
    let name: String = inner.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (!name.is_empty()).then(|| (name.to_ascii_lowercase(), closing))
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img" | "input" | "meta" | "link" | "wbr" | "source")
}

/// The HTML inside the element whose opening tag came just before `rest`.
fn inner_html<'a>(rest: &'a str, name: &str) -> &'a str {
    let after = element_text(rest, name).1;
    let end = rest.len() - after.len();
    // Up to its closing tag (`</name>`), which `after` follows.
    rest[..end].rfind("</").map_or(&rest[..end], |close| &rest[..close])
}

/// The text of the element whose opening tag came just before `rest`, tags dropped (an image
/// by its description) and entities decoded, and what follows its closing tag.
fn element_text<'a>(mut rest: &'a str, name: &str) -> (String, &'a str) {
    let mut text = String::new();
    let mut depth = 1usize;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>').map(|end| start + end) else { break };
        let tag = &rest[start..=end];
        rest = &rest[end + 1..];
        match tag_name(tag) {
            Some((image, false)) if image == "img" => text.push_str(&attribute(tag, "alt").unwrap_or_default()),
            // A pill is one line: a break inside one parts its words with a space.
            Some((line, false)) if line == "br" => text.push(' '),
            Some((tag_name, closing)) if tag_name == name => {
                if closing {
                    depth -= 1;
                    if depth == 0 {
                        return (decode(&text), rest);
                    }
                } else if !tag.ends_with("/>") {
                    depth += 1;
                }
            }
            _ => {}
        }
    }
    text.push_str(rest);
    (decode(&text), "")
}

/// An attribute's decoded value; `Some("")` for one given without a value.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let inner = tag.trim_start_matches('<').trim_end_matches('>').trim_end_matches('/');
    let mut rest = inner.split_once(|c: char| c.is_whitespace())?.1;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return None;
        }
        let key_end = rest.find(|c: char| c == '=' || c.is_whitespace()).unwrap_or(rest.len());
        let key = &rest[..key_end];
        rest = rest[key_end..].trim_start();
        let value = match rest.strip_prefix('=') {
            Some(after) => {
                let after = after.trim_start();
                let (value, remaining) = match after.chars().next() {
                    Some(quote @ ('"' | '\'')) => {
                        let close = after[1..].find(quote).map_or(after.len(), |i| i + 1);
                        (&after[1..close], after.get(close + 1..).unwrap_or(""))
                    }
                    _ => {
                        let close = after.find(char::is_whitespace).unwrap_or(after.len());
                        (&after[..close], &after[close..])
                    }
                };
                rest = remaining;
                decode(value)
            }
            None => String::new(),
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(value);
        }
    }
}

/// `text` with HTML's entities decoded.
fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';').filter(|semi| *semi <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `text` safe inside HTML text and quoted attributes.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mention_of_an_id_with_a_slash_should_link_it_as_one_segment() {
        let link = mention_link("user:@team/bot:example.org").expect("a link");
        assert_eq!(link, "https://matrix.to/#/@team%2Fbot:example.org");
        assert_eq!(
            to_pills(&format!("<a href=\"{link}\">Bot</a>"), Pills::Display),
            "<span data-pill=\"user:@team/bot:example.org\">Bot</span>"
        );
    }

    #[test]
    fn a_room_mention_should_become_a_pill() {
        assert_eq!(
            to_pills("<a href=\"https://matrix.to/#/%23rust%3Amatrix.org\">#rust</a>", Pills::Display),
            "<span data-pill=\"room:#rust:matrix.org\">#rust</span>"
        );
    }

    #[test]
    fn a_matrix_uri_mention_should_become_a_pill() {
        assert_eq!(
            to_pills("<a href='matrix:u/ada:x.org'>Ada</a>", Pills::Display),
            "<span data-pill=\"user:@ada:x.org\">Ada</span>"
        );
    }

    #[test]
    fn a_link_to_a_message_should_stay_a_link() {
        let html = "<a href=\"https://matrix.to/#/!r:x.org/$e\">this</a>";
        assert_eq!(to_pills(html, Pills::Display), html);
    }

    #[test]
    fn a_web_link_should_stay_a_link() {
        let html = "<a href=\"https://example.org\">site</a>";
        assert_eq!(to_pills(html, Pills::Display), html);
    }

    #[test]
    fn a_spoiler_should_become_a_pill_of_its_words() {
        assert_eq!(
            to_pills("<span data-mx-spoiler=\"plot\">he <b>dies</b></span> sadly", Pills::Display),
            "<span data-pill=\"spoiler\">he dies</span> sadly"
        );
    }

    #[test]
    fn nothing_inside_a_spoiler_should_become_a_pill_of_its_own() {
        let html = "<span data-mx-spoiler><img src=\"mxc://x.org/cat\" alt=\":cat:\"> and \
                    <a href=\"https://matrix.to/#/@ada:x.org\">Ada</a> did it</span>";
        assert_eq!(to_pills(html, Pills::Display), "<span data-pill=\"spoiler\">:cat: and Ada did it</span>");
    }

    #[test]
    fn a_line_break_in_a_spoiler_should_part_its_words() {
        assert_eq!(
            to_pills("<span data-mx-spoiler>first<br />second</span>", Pills::Display),
            "<span data-pill=\"spoiler\">first second</span>"
        );
    }

    #[test]
    fn a_spoiler_that_is_also_maths_should_stay_hidden() {
        assert_eq!(
            to_pills("<span data-mx-spoiler data-mx-maths=\"x\">secret</span>", Pills::Display),
            "<span data-pill=\"spoiler\">secret</span>"
        );
    }

    #[test]
    fn a_spoiler_in_a_code_block_should_stay_hidden() {
        assert_eq!(
            to_pills("<pre><code>let <span data-mx-spoiler>secret</span></code></pre>", Pills::Display),
            "<code>let <span data-pill=\"spoiler\">secret</span></code>"
        );
    }

    #[test]
    fn a_mention_around_a_spoiler_should_leave_the_spoiler_hidden() {
        let html = "<a href=\"https://matrix.to/#/@ada:x.org\"><span data-mx-spoiler>secret</span></a> next";
        assert_eq!(to_pills(html, Pills::Display), "<span data-pill=\"spoiler\">secret</span> next");
    }

    #[test]
    fn shown_maths_should_read_as_unicode() {
        assert_eq!(
            to_pills("<span data-mx-maths=\"x^2\"><code>x^2</code></span>", Pills::Display),
            "<span data-pill=\"math:x^2\">x²</span>"
        );
    }

    #[test]
    fn a_formula_block_should_be_a_paragraph_of_its_own() {
        assert_eq!(
            to_pills("<div data-mx-maths=\"\\frac{1}{2}\"><pre><code>\\frac{1}{2}</code></pre></div>", Pills::Display),
            "<p><span data-pill=\"math-block:\\frac{1}{2}\">1/2</span></p>"
        );
    }

    #[test]
    fn edited_maths_should_read_as_its_latex() {
        assert_eq!(
            to_pills("<span data-mx-maths=\"a &lt; b\"><code>a &lt; b</code></span>", Pills::Compose),
            "<span data-pill=\"math\">a &lt; b</span>"
        );
    }

    #[test]
    fn a_custom_emoji_should_become_a_pill_named_by_its_short_code() {
        assert_eq!(
            to_pills(
                "hi <img data-mx-emoticon src=\"mxc://x.org/cat\" alt=\":blobcat:\" height=\"32\"> there",
                Pills::Display
            ),
            "hi <span data-pill=\"emote:mxc://x.org/cat\">:blobcat:</span> there"
        );
    }

    #[test]
    fn a_custom_emoji_the_sanitiser_left_unmarked_should_still_become_a_pill() {
        assert_eq!(
            to_pills("<img src=\"mxc://x.org/cat\" alt=\":blobcat:\" height=\"32\">", Pills::Display),
            "<span data-pill=\"emote:mxc://x.org/cat\">:blobcat:</span>"
        );
    }

    #[test]
    fn an_image_from_the_web_should_show_its_description() {
        assert_eq!(to_pills("<img src=\"https://x.org/a.png\" alt=\"a cat\">", Pills::Display), "a cat");
    }

    #[test]
    fn a_spoiler_pill_should_go_as_a_spoiler() {
        assert_eq!(
            from_pills("<p><span data-pill=\"spoiler\">he <strong>dies</strong></span></p>"),
            "<p><span data-mx-spoiler>he <strong>dies</strong></span></p>"
        );
    }

    #[test]
    fn a_custom_emoji_pill_should_go_as_an_image() {
        assert_eq!(
            from_pills("<p><span data-pill=\"emote:mxc://x.org/cat\">:blobcat:</span></p>"),
            "<p><img data-mx-emoticon src=\"mxc://x.org/cat\" alt=\":blobcat:\" title=\":blobcat:\" height=\"32\" /></p>"
        );
    }

    #[test]
    fn a_formula_pill_should_go_as_maths_with_its_latex_as_the_fallback() {
        assert_eq!(
            from_pills("<p><span data-pill=\"math\">a &lt; b</span></p>"),
            "<p><span data-mx-maths=\"a &lt; b\"><code>a &lt; b</code></span></p>"
        );
    }

    #[test]
    fn an_everyone_pill_should_go_as_plain_text() {
        assert_eq!(from_pills("<p><span data-pill=\"everyone\">@room</span> hi</p>"), "<p>@room hi</p>");
    }

    #[test]
    fn a_message_should_come_back_from_its_pills_as_it_went() {
        let html =
            "<p>hi <a href=\"https://matrix.to/#/@ada:x.org\">Ada</a> and <span data-mx-spoiler>secret</span></p>";
        assert_eq!(from_pills(&to_pills(html, Pills::Compose)), html);
    }

    #[test]
    fn a_pill_inside_a_formatted_span_should_close_in_order() {
        assert_eq!(
            from_pills("<span style=\"x\"><span data-pill=\"user:@a:x\">A</span></span>"),
            "<span style=\"x\"><a href=\"https://matrix.to/#/@a:x\">A</a></span>"
        );
    }
}
